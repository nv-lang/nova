# План 05: `as`-cast — реализация narrowing в codegen

**Статус:** ✅ выполнено (2026-05-08).
**Дата создания:** 2026-05-08.
**Зависимости:** [D54](../../spec/decisions/03-syntax.md#d54) — спека уже
описывает семантику.

**Результат:** Ф.1-Ф.5 закрыты. `nova_tests/syntax/as_cast.nv` 8/8 PASS,
63/63 nova_tests PASS, stdlib без регрессий (crc32/fnv продолжают
работать). Подробнее — `simplifications.md` запись от 2026-05-08
[P-as-cast-wraparound].

---

## Проблема

В текущей bootstrap-реализации (`compiler-codegen/src/codegen/emit_c.rs`)
оператор `as` работает как **no-op**:

```rust
ExprKind::As(inner, _ty) => {
    // Type cast — emit without cast for now (types are compatible in C)
    self.emit_expr(inner)
}
```

`_ty` (целевой тип) игнорируется. Inner expression эмитится «как есть».

### Что ломается

#### Сценарий 1 — narrowing с независимым результатом

```nova
let a int = 1111
let b = a as byte
```

**Ожидание (по D54):** `b` имеет тип `byte` со значением `1111 mod 256 = 87`.

**Реальность:** codegen эмитит `nova_int b = a`, тип `b` остаётся `nova_int = int64_t`, значение **1111**.

Никакого truncate'а не происходит, потому что:
- Codegen не вставляет C-cast.
- Тип `b` инферится из RHS, а RHS = `inner` без cast'а.
- C-компилятор не делает narrowing — оба `nova_int`.

#### Сценарий 2 — bitwise операции с cast'ом

```nova
let n int = 0xFFFF
let lo = (n & 0xFF) as byte
let combined = (lo as int) << 8 | (lo as int)
```

`combined` может содержать неожиданные верхние биты, потому что `lo as byte` не truncate'ил, а `lo as int` — снова no-op.

#### Сценарий 3 — где **работает** случайно

```nova
let arr []byte = []
arr.push(a as byte)        // OK — C narrowing при push в uint8_t-слот
fn f(b byte) { ... }
f(a as byte)               // OK — C narrowing при передаче параметра
```

Здесь C-сторона видит target-тип (`uint8_t` массива/параметра) и делает implicit narrowing на копировании. Но это **косвенное** срабатывание, не следствие `as`.

### Почему это bug относительно spec'а

D54 явно говорит:

> **`as`** — compile-time конвертация значения между совместимыми
> типами (numeric cast, newtype ↔ underlying, sum → int).
> **Возвращает значение целевого типа.**

Целевой тип сейчас **не возвращается** — возвращается тип inner-expression. Это:
- Ломает type-inference в `let b = ...` (b infer'ится не в byte).
- Ломает `if b is byte { ... }` сразу после cast'а.
- Создаёт незаметную зависимость от контекста использования (truncate работает только если target-тип «прорастает» сверху через C-narrowing).

---

## Цель

`as` в codegen эмитит **явный C-cast** к целевому типу. Тип результата
выражения `<inner> as <T>` становится `T`, не type-of(inner).

Поведение для numeric cast'ов согласуется с D54:
- **Wraparound** для int → меньший int (truncate младших битов).
- **Truncate** для f64 → int (как в C).
- **Identity** для совместимых пар (int → f64, byte → int).
- **Newtype ↔ underlying** — no-op в C (одинаковое представление).

---

## Не цель

- **Checked cast'ы** (panic на overflow) — отложены до Q-checked-cast в open-questions.
- **TryFrom**-альтернатива — это D77 (отдельный механизм).
- **Sum → int** через `as` — уже работает (sum-конструкторы инициализируются числом, `as int` возвращает discriminant). Не трогаем.
- **`any → T`** через `as` — запрещено по D54, не реализуем.

---

## Что делаем

### Ф.1 — codegen `ExprKind::As` эмитит C-cast

В `compiler-codegen/src/codegen/emit_c.rs`:

```rust
ExprKind::As(inner, ty) => {
    let c_ty = self.type_ref_to_c(ty)?;     // TypeRef → "nova_byte" / "nova_f64" / ...
    let v = self.emit_expr(inner)?;
    Ok(format!("(({}){})", c_ty, v))
}
```

Существующий helper `type_ref_to_c` уже умеет мапить `byte` → `nova_byte`,
`f64` → `nova_f64` и т.д. (строки 558, 2378, 3898, 6135 в `emit_c.rs`).

### Ф.2 — type inference для `let b = expr as T`

В `infer_expr_c_type` для `ExprKind::As(_, ty)` возвращать тип
из target-аннотации, а не из inner-expression. Сейчас inference,
видимо, через inner; нужно явно учесть `As`.

### Ф.3 — newtype ↔ underlying — no-op в C

Для `type UserId u64; let u = 42 as UserId` нет реальной
narrow-конверсии — UserId и u64 представлены как `nova_int` оба.
Cast эмитится `((nova_int)(42))` — это идempotent, корректно.

### Ф.4 — тесты в `nova_tests/`

Новый файл `nova_tests/syntax/as_cast.nv`:

```nova
test "as byte truncates" {
    let a int = 1111
    let b = a as byte
    assert(b as int == 87)        // 1111 mod 256
}

test "as byte chained with bitwise" {
    let n int = 0xFFFF
    let lo = (n & 0xFF) as byte
    assert(lo as int == 0xFF)
}

test "as i32 from i64 wraparound" {
    let big int = 0x1_0000_0001
    let small = big as i32
    assert(small as int == 1)      // младшие 32 бита
}

test "as f64 from int" {
    let n int = 42
    let f = n as f64
    assert(f == 42.0)
}

test "as int from f64 truncates" {
    let f f64 = 3.7
    let n = f as int
    assert(n == 3)                  // truncate, не round
}

test "newtype as underlying — identity" {
    let id UserId = 42 as UserId
    let n int = id as int
    assert(n == 42)
}
```

### Ф.5 — sweep stdlib

После Ф.1-Ф.4 проверить либы которые делали ручной shift-and-mask
вместо `as byte`:
- crypto/md5, crypto/sha1, crypto/sha256, crypto/hmac
- checksums/crc32, checksums/fnv
- identifiers/ulid, identifiers/uuid

Многие из них имеют код вида `((v >> 24) & 0xFF) as byte` — после
фикса `as` будет достаточно `(v >> 24) as byte` (старшие биты
обрезаются автоматически). Это **косметика, не блокер** —
старая форма продолжит работать.

---

## Acceptance criteria

- ✅ `let a int = 1111; let b = a as byte; b as int == 87`.
- ✅ Тип `b` в codegen — `nova_byte`, не `nova_int`.
- ✅ `nova_tests/syntax/as_cast.nv` — все тесты PASS.
- ✅ Все существующие тесты `nova_tests/` PASS (нет регрессий).
- ✅ Все либы в `std/` где использовался `as byte` продолжают работать.

---

## Trade-offs / упрощения

### Wraparound vs panic-on-overflow

D54 не уточняет поведение overflow. Текущее предложение — **wraparound**
(C-style narrowing). Это согласовано с прецедентом C/Go/Rust (Rust
до 1.45 был UB, после — wraparound в release). Альтернатива
panic-on-overflow требует runtime check'ов — для bootstrap преждевременно.

Запишу в `simplifications.md` как [P-as-cast-wraparound] — пока wraparound,
checked-cast в будущем через D-decision.

### Не трогаем `is`

`is` — это runtime type-check для `any` и variant-check для sum
(D54 v2). Реализован отдельно (`emit_c.rs:3405`), работает корректно.
Эта задача только про `as`.

---

## План работ

1. **Ф.1** — `emit_c.rs::ExprKind::As` эмитит `((<c_ty>)(<inner>))`.
2. **Ф.2** — `infer_expr_c_type` возвращает target-тип для `As`.
3. **Ф.3** — newtype/alias корректно мапятся в underlying C-тип
   (вероятно уже работает через существующий `type_ref_to_c`).
4. **Ф.4** — `nova_tests/syntax/as_cast.nv` (6+ тестов выше).
5. **Ф.5** — smoke-проверка в либах `std/` где встречается `as byte`.
6. **Записать** в `simplifications.md` пометку про wraparound.

---

## Оценка

Малый patch (3-5 строк codegen + 1 строка inference) + тесты.
Полдня работы для компилятор-агента.

---

## Ссылки

- [spec/decisions/03-syntax.md → D54](../../spec/decisions/03-syntax.md#d54)
  — семантика `as` и `is`.
- [spec/decisions/03-syntax.md → D44](../../spec/decisions/03-syntax.md#d44)
  — числовые литералы и numeric promotion.
- [spec/decisions/02-types.md → D52](../../spec/decisions/02-types.md#d52)
  — newtype-объявления.
- `compiler-codegen/src/codegen/emit_c.rs:3400-3403` — текущий
  no-op cast.
- `compiler-codegen/nova_rt/nova_rt.h:36` — `typedef uint8_t nova_byte`.
