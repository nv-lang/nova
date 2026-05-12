// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 38: Numeric type constants (`int.MAX` / `f64.MAX` / etc.)

> **Статус:** план, не начат. Средний приоритет.
> **Создан:** 2026-05-12.
> **Обнаружен:** 2026-05-12 при работе над Plan 35 Ф.1 (cross-file
> resolve) — `std/collections/range.nv:Range.inclusive` использует
> `int.MAX`, codegen эмитит undefined C identifier `int_MAX`.
> **Блокер для:** full compile `std/collections/range.nv` через
> `nova build` (после Plan 35 cross-file resolve).

---

## Контекст

Spec D26 prelude декларирует numeric type constants:

```nova
int.MAX                            // максимальное значение i64
int.MIN                            // минимальное
i32.MAX / i32.MIN
u8.MAX / u32.MAX / u64.MAX
f64.MAX / f64.MIN_POSITIVE / f64.NAN / f64.INFINITY / f64.EPSILON / f64.PI / f64.E
f32.MAX / f32.NAN / etc.
byte.MAX                           // 255
```

Spec: [spec/decisions/08-runtime.md:1741](../../spec/decisions/08-runtime.md).

## Проблема

`std/collections/range.nv:Range.inclusive` использует `int.MAX`:

```nova
export fn Range.inclusive(start int, end int) Fail[OverflowError] -> Self {
    if end == int.MAX {                                  // ← здесь
        throw OverflowError { msg: "Range.inclusive(_, int.MAX): cannot normalize to half-open" }
    }
    { start, end: end + 1 }
}
```

**Codegen output** (`std/collections/range.c`):
```c
if (end == int_MAX) {              // ← undefined C identifier!
    ...
}
```

C-компиляция: `error: use of undeclared identifier 'int_MAX'`.

## Root cause

В `compiler-codegen/src/codegen/emit_c.rs` — `ExprKind::Path` для двухкомпонентного path `[primitive_type, constant_name]` (например `int.MAX`):

- **Type-check phase:** path резолвится как `Member { obj: Ident("int"), name: "MAX" }` или `Path(["int", "MAX"])`. Парсер уже special-case'ит primitive types (Plan 08 Ф.2) — `int.try_from(...)` работает. Но **constants на primitive types не зарегистрированы** в codegen.
- **Codegen phase:** mangling `<path>_<name>` → `int_MAX` без awareness что это **type-level constant**, который нужно map'нуть на C-runtime constant (`INT64_MAX` из `<stdint.h>`).

Аналогично у `f64.MAX` будет `f64_MAX` undefined. Это **системный gap** — numeric type constants полностью не работают в codegen.

## Real-world impact

**Файлы которые сейчас падают на этом:**
- `std/collections/range.nv` — `Range.inclusive` (используется в `RangeIter`/`StepRangeIter` infrastructure).

**Файлы которые потенциально пострадают** (нужен grep по std/+examples):
- Numeric stdlib (когда появится — `std/numeric/clamp`, `std/numeric/bounded`).
- Any user code использующий `int.MAX` / `f64.NAN` etc.

## Scope

### Ф.1 — Codegen mapping для known constants

Special-case в `emit_c.rs::emit_path_or_member_call` (или где резолвятся `Path([prim, const])`):

| Nova path | C output |
|---|---|
| `int.MAX` | `((nova_int)INT64_MAX)` |
| `int.MIN` | `((nova_int)INT64_MIN)` |
| `i64.MAX` | `INT64_MAX` |
| `i64.MIN` | `INT64_MIN` |
| `i32.MAX` | `INT32_MAX` |
| `i32.MIN` | `INT32_MIN` |
| `i16.MAX` / `.MIN` | `INT16_MAX` / `INT16_MIN` |
| `i8.MAX` / `.MIN` | `INT8_MAX` / `INT8_MIN` |
| `u64.MAX` | `UINT64_MAX` |
| `u32.MAX` | `UINT32_MAX` |
| `u16.MAX` | `UINT16_MAX` |
| `u8.MAX` | `UINT8_MAX` |
| `byte.MAX` | `((nova_byte)UINT8_MAX)` |
| `f64.MAX` | `DBL_MAX` (`<float.h>`) |
| `f64.MIN_POSITIVE` | `DBL_MIN` |
| `f64.NAN` | `NAN` (`<math.h>`) |
| `f64.INFINITY` | `INFINITY` |
| `f64.NEG_INFINITY` | `(-INFINITY)` |
| `f64.EPSILON` | `DBL_EPSILON` |
| `f64.PI` | `3.14159265358979323846` (no C standard PI const, use literal) |
| `f64.E` | `2.71828182845904523536` |
| `f32.MAX` | `FLT_MAX` |
| `f32.NAN` | `((float)NAN)` |
| etc. |  |

**`bool.MIN`/`MAX`?** — false/true. Не emit'ить — это не type-constant convention.

**`char`?** — codepoint range. `char.MAX = 0x10FFFF`, `char.MIN = 0`.

### Ф.2 — Type-check side

`infer_expr_c_type` для `Path([prim, "MAX"])` или `Member { obj: Ident("int"), name: "MAX" }`:
- Если `prim` ∈ {int, i8-i64, u8-u64} → `nova_int` (для signed) / `uint64_t` (для unsigned).
- Если `prim` ∈ {f32, f64} → `nova_f64` / `nova_f32`.
- Если `prim == "byte"` → `nova_byte` (uint8_t).
- Если `prim == "char"` → `nova_int` (codepoint).

### Ф.3 — Constants на primitive types в `is_known` / name resolution

`compiler-codegen/src/types/mod.rs::is_known` — type-check должен пропускать `int.MAX` как valid path. Сейчас если path не resolved, fall through к "undefined identifier". Добавить special-case:
- `Path([prim, const_name])` где `prim` — primitive type из baseline и `const_name` ∈ {MAX, MIN, MIN_POSITIVE, EPSILON, NAN, INFINITY, NEG_INFINITY, PI, E} → valid.

### Ф.4 — Tests

`nova_tests/types/numeric_constants.nv` (новый):

```nova
test "int.MAX is positive" {
    assert(int.MAX > 0)
    assert(int.MAX > 1000000)
}

test "int.MIN is negative" {
    assert(int.MIN < 0)
    assert(int.MIN < -1000000)
}

test "int.MAX + 1 wraps" {
    // 2's complement: MAX + 1 = MIN.
    let v = int.MAX + 1
    assert(v == int.MIN)
}

test "u8.MAX = 255" {
    assert(u8.MAX == 255)
}

test "f64.NAN != NaN" {
    let n = f64.NAN
    assert(n != n)
}

test "f64.INFINITY > MAX" {
    assert(f64.INFINITY > f64.MAX)
}
```

Дополнительно: **regression** — `nova build std/collections/range.nv` собирается (закрывает Plan 35 Ф.1 blocker).

### Ф.5 — Spec update

`spec/decisions/08-runtime.md` D26 — добавить таблицу numeric constants с C-mapping (информативно). Закрыть как «реализовано».

---

## Acceptance criteria

- `nova check tmp_int_max.nv` (с `int.MAX` expression) — exit 0.
- `nova build tmp_int_max.nv` — exit 0, exe runs корректно.
- `nova test nova_tests/types/numeric_constants.nv` — все ассерты PASS.
- `nova build std/collections/range.nv` — exit 0 (после Plan 35 Ф.1
  cross-file resolve работает).
- 208/208 existing tests — без regression.

---

## Связь

- **Plan 35** — Plan 38 разблокирует `std/collections/range.nv` full
  compile (текущий blocker после Plan 35 Ф.1 inline expansion работает).
- **Plan 08 Ф.2** — already adds `int.try_from` / etc. как primitive
  path. Plan 38 продолжает паттерн для **constants** на primitive
  types.
- **Spec D26** — type-constants part of prelude.

---

## Что НЕ входит

- Custom type constants на user types (например `MyType.DEFAULT_SIZE`).
  Это D41 territory (no static state, но const на типе разрешается —
  отдельный feature).
- `min`/`max` функции (это runtime, не constants). Уже работают через
  `std/runtime/math.nv`.
- `int.BITS` / `i64.BITS` constants (Rust convention). Если нужно —
  добавить отдельным D-блоком.

---

## Estimate

**~60 LOC** в `emit_c.rs` (special-case dispatch) + **~30 LOC** в
`types/mod.rs` (is_known) + **~80 LOC** тесты. **Полдня** работы.

---

## Риски

- **Float constants без C standard.** `M_PI` это POSIX extension,
  не C standard. Используем literal `3.14159265358979323846` — теряем
  bit-exact представление? Mitigation: use `__builtin_constant_p` или
  pre-compute через `nova_f64` constant. Минорный gap.
- **Mapping consistency.** Если codegen эмитит `INT64_MAX`, а runtime
  define'ит `nova_int = int64_t` — все OK. Если runtime сменит alias
  (например `nova_int = i128`) — нужно обновить mapping. Mitigation:
  emit через `((nova_int){c_constant})LL` cast.
- **Edge: `int.MAX + 1` в const-init**. Compile-time overflow в C
  literal. Mitigation: emit как `((nova_int){c_literal}LL)` где Rust
  pre-computes overflow поведение. Можно отложить если bootstrap не
  имеет const-init eval.

---

## Audit history

- **2026-05-12 v1:** создан после Plan 35 Ф.1 MVP — `range.nv` блокирует
  full compile из-за `int.MAX` undefined. Это **отдельный gap**, не
  Plan 35 territory.
