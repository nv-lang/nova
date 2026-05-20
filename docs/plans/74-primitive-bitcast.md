# Plan 74: Primitive bitcast — `to_bits` / `from_bits`

> **Создан 2026-05-19.**
>
> **Цель:** добавить безопасные методы reinterpret-cast для примитивных
> value-типов: `f64 ↔ u64`, `f32 ↔ u32`. Нужны для IEEE 754 bit-level
> операций (хэши, NaN-boxing, FFI, `realtime nogc` зоны).

---

## Контекст

В Rust это `f64::to_bits()` / `f64::from_bits()` — внутри `transmute`.
В Nova GC-объектам transmute не нужен, но для **float-примитивов**
reinterpret-cast — легитимная операция. Оформляем как именованные
методы, без общего `unsafe transmute`.

Для целочисленных пар (`i64 ↔ u64`, `i32 ↔ u32`) reinterpret не нужен —
там достаточно `as`-cast (биты те же, two's complement).

---

## API

```nova
// f64 ↔ u64
export external fn f64 @to_bits() -> u64
export external fn f64.from_bits(bits u64) -> f64

// f32 ↔ u32
export external fn f32 @to_bits() -> u32
export external fn f32.from_bits(bits u32) -> f32
```

Пример:
```nova
let f = 1.0
let bits = f.to_bits()           // 0x3FF0000000000000
let back = f64.from_bits(bits)   // 1.0
```

---

## Фазы

### Ф.1 — C-рантайм: реализация в `nova_rt`

**`compiler-codegen/nova_rt/`** — новый файл `numeric.h` или дополнение существующего:

```c
static inline uint64_t Nova_f64_to_bits(double v) {
    uint64_t r; memcpy(&r, &v, 8); return r;
}
static inline double Nova_f64_from_bits(uint64_t b) {
    double r; memcpy(&r, &b, 8); return r;
}
static inline uint32_t Nova_f32_to_bits(float v) {
    uint32_t r; memcpy(&r, &v, 4); return r;
}
static inline float Nova_f32_from_bits(uint32_t b) {
    float r; memcpy(&r, &b, 4); return r;
}
```

`memcpy` — единственный портабельный способ (UB-safe, компилятор
оптимизирует до одной инструкции).

### Ф.2 — Регистрация в `runtime_registry.rs`

**`compiler-codegen/src/codegen/runtime_registry.rs`** — добавить
4 записи по аналогии с остальными external fn методами.

### Ф.3 — Nova-стабы (автогенерация)

Обновить `std/runtime/` (или добавить `std/runtime/numeric.nv`) с
декларациями:

```nova
module runtime.numeric

export external fn f64 @to_bits() -> u64
export external fn f64.from_bits(bits u64) -> f64
export external fn f32 @to_bits() -> u32
export external fn f32.from_bits(bits u32) -> f32
```

### Ф.4 — Тесты

```nova
// bitcast-ok.nv
let bits = (1.0).to_bits()
assert bits == 0x3FF0000000000000

let back = f64.from_bits(0x3FF0000000000000)
assert back == 1.0

let neg_zero = f64.from_bits(0x8000000000000000)
assert neg_zero == 0.0  // -0.0 == 0.0 по IEEE 754
```

---

## Критические файлы

| Файл | Изменение |
|------|-----------|
| `compiler-codegen/nova_rt/numeric.h` (новый) | C inline реализация |
| `compiler-codegen/src/codegen/runtime_registry.rs` | Регистрация 4 методов |
| `std/runtime/numeric.nv` (новый) | Nova-декларации |
| `tests/bitcast-ok.nv` | Тест |

## Верификация

```
nova test tests/bitcast-ok.nv
nova test std/
```
