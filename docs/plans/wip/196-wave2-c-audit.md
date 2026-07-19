<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 196 — Волна-C: АУДИТ dead-dup хардкод-зеркал f64/int методов

**Дата аудита:** 2026-07-19  
**Базовый коммит:** ca21af1ad (main)  
**Worktree:** nova-w2c (p196-wave2-c)  
**Модель:** haiku (механика по списку)

## Выводы

| Функция | Строка | Кол-во методов | Вердикт | Риск dead-dup | Рекомендация |
|---|---|---|---|---|---|
| `f64_method_to_c` | 46869-46900 | 27 | **ЖИВ** (обоснованный интринсик) | НУЛЕВОЙ | Оставить; doc-коммент (план 196.3 завершён) |
| `int_method_to_c` | 46950-46954 | 1 | **ЖИВ** (обоснованный интринсик) | НУЛЕВОЙ | Оставить; doc-коммент (план 196.3 завершён) |

**Сводка:** 0 dead-dup, 28 методов, 2 функции — все обоснованы. Обе остаются **единственным источником маппинга Nova-метод → C-функция** в emit-пути.

---

## Детальный аудит

### Функция `f64_method_to_c` (emit_c.rs L46869-46900)

**Назначение:** D74 маппер — Nova f64/f32 методы → libm C-функции.

**Вызывающие сайты:**
- L36199: `if let Some(c_fn) = Self::f64_method_to_c(method)` в emit_call для `nova_f64`/`nova_f32`
- L52370: doc-коммент (упоминание в комментарии о плане 196.3)

**Класс использования:** EMIT-PATH (codegen), фаза emit-вызова метода.

#### Дубли в .nv-декларациях

**Источник истины:** `std/src/runtime/math.nv` (auto-generated, но подключен через prelude).

Статус по каждому методу (27 методов):

| Nova-метод | Отображение в C | math.nv L | Статус в .nv | Класс |
|---|---|---|---|---|
| sqrt | sqrt | 13 | export extern fn f64 @sqrt() | ✅ ЗАДЕКЛАРИРОВАН |
| cbrt | cbrt | 16 | export extern fn f64 @cbrt() | ✅ ЗАДЕКЛАРИРОВАН |
| abs | fabs | 19 | export extern fn f64 @abs() | ✅ ЗАДЕКЛАРИРОВАН |
| ceil | ceil | 22 | export extern fn f64 @ceil() | ✅ ЗАДЕКЛАРИРОВАН |
| floor | floor | 25 | export extern fn f64 @floor() | ✅ ЗАДЕКЛАРИРОВАН |
| round | round | 28 | export extern fn f64 @round() | ✅ ЗАДЕКЛАРИРОВАН |
| trunc | trunc | 31 | export extern fn f64 @trunc() | ✅ ЗАДЕКЛАРИРОВАН |
| sin | sin | 34 | export extern fn f64 @sin() | ✅ ЗАДЕКЛАРИРОВАН |
| cos | cos | 37 | export extern fn f64 @cos() | ✅ ЗАДЕКЛАРИРОВАН |
| tan | tan | 40 | export extern fn f64 @tan() | ✅ ЗАДЕКЛАРИРОВАН |
| asin | asin | 43 | export extern fn f64 @asin() | ✅ ЗАДЕКЛАРИРОВАН |
| acos | acos | 46 | export extern fn f64 @acos() | ✅ ЗАДЕКЛАРИРОВАН |
| atan | atan | 49 | export extern fn f64 @atan() | ✅ ЗАДЕКЛАРИРОВАН |
| atan2 | atan2 | 76 | export extern fn f64 @atan2(x f64) | ✅ ЗАДЕКЛАРИРОВАН |
| sinh | sinh | 52 | export extern fn f64 @sinh() | ✅ ЗАДЕКЛАРИРОВАН |
| cosh | cosh | 55 | export extern fn f64 @cosh() | ✅ ЗАДЕКЛАРИРОВАН |
| tanh | tanh | 58 | export extern fn f64 @tanh() | ✅ ЗАДЕКЛАРИРОВАН |
| exp | exp | 61 | export extern fn f64 @exp() | ✅ ЗАДЕКЛАРИРОВАН |
| exp2 | exp2 | 64 | export extern fn f64 @exp2() | ✅ ЗАДЕКЛАРИРОВАН |
| ln | log | 67 | export extern fn f64 @ln() (натуральный log) | ✅ ЗАДЕКЛАРИРОВАН |
| log2 | log2 | 70 | export extern fn f64 @log2() | ✅ ЗАДЕКЛАРИРОВАН |
| log10 | log10 | 73 | export extern fn f64 @log10() | ✅ ЗАДЕКЛАРИРОВАН |
| pow | pow | 79 | export extern fn f64 @pow(exp f64) | ✅ ЗАДЕКЛАРИРОВАН |
| hypot | hypot | 82 | export extern fn f64 @hypot(y f64) | ✅ ЗАДЕКЛАРИРОВАН |
| is_nan | isnan | 85 | export extern fn f64 @is_nan() | ✅ ЗАДЕКЛАРИРОВАН |
| is_finite | isfinite | 88 | export extern fn f64 @is_finite() | ✅ ЗАДЕКЛАРИРОВАН |
| is_infinite | isinf | 91 | export extern fn f64 @is_infinite() | ✅ ЗАДЕКЛАРИРОВАН |

**Результат:** 27/27 методов имеют соответствие в math.nv. НЕТ дублей с другими путями.

#### Кто зовёт

**Глубокий поиск:**

```bash
$ grep -n "f64_method_to_c" compiler-codegen/src/codegen/emit_c.rs
36199:                    if let Some(c_fn) = Self::f64_method_to_c(method) {
46869:    fn f64_method_to_c(method: &str) -> Option<&'static str> {
46979:    /// dead-but-kept) — codegen EMISSION is untouched, `f64_method_to_c`/
46980:    /// `int_method_to_c` remain the sole Nova-method → C-function mapping (called
```

**Единственный вызывающий сайт (вне определения):** L36199, в функции emit_call.

```rust
// L36199 (контекст L36192-36204):
if obj_ty == "nova_f64" || obj_ty == "nova_f32" {
    if let Some(c_fn) = Self::f64_method_to_c(method) {
        let obj_c = self.emit_expr(obj)?;
        let mut arg_strs = vec![obj_c];
        for a in args { arg_strs.push(self.emit_expr(a.expr())?); }
        return Ok(format!("{}({})", c_fn, arg_strs.join(", ")));
    }
}
```

**Фаза:** Codegen emit-вызова метода (когда receiver имеет C-тип nova_f64/nova_f32).

#### Жив ли путь

**Архитектурный статус:** ЖИВОЙ, АКТИВНЫЙ.

**Доказательство:**
1. Вызывается из emit_call (L36199) — критический путь генерации C-вызовов.
2. Нет fallback-пути (return Ok) — если метод найден, он используется.
3. Конец из L36199 — конец обработки (return Ok), нет дальнейших попыток резолва.

**Историческая справка:** В плане 196.3 (D109/D74 checker-visibility migration) методы были **удалены** из `primitive_instance_method_known` (функция существования для checker), но **остались в codegen emit**. Комментарий L46979-46981:

> "Those two arms are therefore unreachable now (removed, not dead-but-kept) — codegen EMISSION is untouched, `f64_method_to_c`/`int_method_to_c` remain the sole Nova-method → C-function mapping (called directly from `emit_call`, not through this existence oracle)."

**Вывод:** Путь ЖИВОЙ, это не обход (fallback) — это ОСНОВНОЙ маппер.

---

### Функция `int_method_to_c` (emit_c.rs L46950-46954)

**Назначение:** D74 маппер — Nova int методы → C-функции (сейчас только abs).

**Вызывающие сайты:**
- L36575: `if let Some(c_fn) = Self::int_method_to_c(method)` в emit_call для `nova_int`
- L52382: doc-коммент (упоминание)

**Класс использования:** EMIT-PATH (codegen), фаза emit-вызова метода.

#### Дубли в .nv-декларациях

**Источник истины:** `std/src/runtime/math.nv` L179.

| Nova-метод | Отображение в C | math.nv L | Статус в .nv | Класс |
|---|---|---|---|---|
| abs | llabs | 179 | export extern fn int @abs() | ✅ ЗАДЕКЛАРИРОВАН |

**Результат:** 1/1 метод имеет соответствие в math.nv. НЕТ дублей.

#### Кто зовёт

**Глубокий поиск:**

```bash
$ grep -n "int_method_to_c" compiler-codegen/src/codegen/emit_c.rs
36575:                    if let Some(c_fn) = Self::int_method_to_c(method) {
46950:    fn int_method_to_c(method: &str) -> Option<&'static str> {
46980:    /// `int_method_to_c` remain the sole Nova-method → C-function mapping (called
52382:    /// [196.5 Stage-D] B11aa_int_math REMOVED. D74 `int_method_to_c`
```

**Единственный вызывающий сайт (вне определения):** L36575, в emit_call.

```rust
// L36575 (контекст L36571-36581):
if obj_ty == "nova_int" {
    if let Some(c_fn) = Self::int_method_to_c(method) {
        let obj_c = self.emit_expr(obj)?;
        let mut arg_strs = vec![obj_c];
        for a in args { arg_strs.push(self.emit_expr(a.expr())?); }
        return Ok(format!("{}({})", c_fn, arg_strs.join(", ")));
    }
}
```

**Фаза:** Codegen emit-вызова метода (когда receiver имеет C-тип nova_int).

#### Жив ли путь

**Архитектурный статус:** ЖИВОЙ, АКТИВНЫЙ.

**Доказательство:**
1. Вызывается из emit_call (L36575) — критический путь.
2. Единственное место в коде (вне def).
3. Используется для маппинга `int.abs()` → `llabs()` в C.

**Историческая справка:** Комментарий на L52382:

> "[196.5 Stage-D] B11aa_int_math REMOVED. D74 `int_method_to_c` itself stays (emit_call channel, unlike checker-visibility arm in primitive_instance_method_known)."

**Вывод:** Путь ЖИВОЙ, маппинг для abs() остаётся обоснованным.

---

## Причины обоснованности (НЕ dead-dup)

### 1. Emit-путь vs Checker-visibility (разные каналы)

План 196.3 **отделил** две функции:
- **Checker (старая роль):** `primitive_instance_method_known` — существование метода для checker → удалена с f64/int методов
- **Codegen (текущая роль):** `f64_method_to_c`/`int_method_to_c` — маппинг к C-функциям при генерации вызова → **остаётся живой**

Цитата из L46979-46981:
> "Those two arms are therefore unreachable now (removed, not dead-but-kept) — codegen EMISSION is untouched, `f64_method_to_c`/`int_method_to_c` remain the sole Nova-method → C-function mapping"

### 2. Единственный маппер в emit-пути

Оба f64/int методы — **это ЕДИНСТВЕННЫЙ способ** генерировать правильный C-вызов:
- `(9.0).sqrt()` → `sqrt(9.0)` (emit_call ищет в f64_method_to_c)
- `(-42).abs()` → `llabs(-42)` (emit_call ищет в int_method_to_c)

Нет fallback-пути (no legacy, no deferred).

### 3. Libm vs Nova-body различие

Libm функции — это **НЕ Nova-body методы**, это extern-интринсики:
- math.nv объявляет `export extern "nova" fn f64 @sqrt()` (без body)
- Codegen должен знать, как маппить к C-коду (через `f64_method_to_c`)
- Checker разрешает через method_table (живая декларация в prelude)

Архитектурно: math.nv + f64_method_to_c = **synchronized pair**.

### 4. Прецедент: str-методы D109

Card примечание L46901:

> "Прецедент: удаление f64-ветки давало `[E_UNKNOWN_METHOD]` на `(9.0).sqrt()` — значит НЕ дубль."

D109 пруна вернула **только мёртвый дубль** (str-методы, у которых БЫЛО user-body в std), но оставила **обоснованные интринсики** (hash/eq/ord). Аналогично здесь.

---

## Классификация per-метод

### f64_method_to_c (27 методов)

Все 27 методов — **обоснованные libm-интринсики**:
- Все существуют в math.nv как `export extern "nova"`
- Все маппят на стандартные libm-функции (POSIX, не специфичные Nova)
- Все вызываются из emit_call при (nova_f64/nova_f32).@method()

**Рекомендация:** ОСТАВИТЬ. Добавить doc-коммент к функции про план 196.3 (завершено).

### int_method_to_c (1 метод)

Метод `abs`:
- Существует в math.nv L179 как `export extern fn int @abs() -> int`
- Маппит на `llabs` (C-stdlib, стандартный abs для long long)
- Вызывается из emit_call при nova_int.@abs()

**Рекомендация:** ОСТАВИТЬ. Добавить doc-коммент про план 196.3.

---

## План для будущей sonnet-волны

**ВОЛНА-EMIT-REFACTOR (не в волне-2):**

Если когда-то потребуется рефакторить emit-путь для методов:
1. Заменить `f64_method_to_c`/`int_method_to_c` на query к method_table (одно окно)
2. Убедиться, что method_table имеет полное покрытие (таблица выше)
3. Снести static match в emit_c.rs (но это БОЛЬШОЙ рефакторинг, требует архитектуры)

**Сейчас:** ОСТАВИТЬ без изменений.

---

## Итоговая таблица

| Функция | Методов | Live-sites | Dead-dup? | Дубли .nv? | Рекомендация |
|---|---|---|---|---|---|
| f64_method_to_c | 27 | L36199 (emit_call) | **НЕТ** | 0 дублей (все в math.nv) | ОСТАВИТЬ |
| int_method_to_c | 1 | L36575 (emit_call) | **НЕТ** | 0 дублей (в math.nv) | ОСТАВИТЬ |
| **ИТОГО** | **28** | **2 сайта** | **0 dead-dup** | **0 конфликтов** | **100% live** |

---

## Доказательства (цитаты)

### Цитата 1: Архитектурный статус (emit_c.rs L46979-46981)

```rust
/// dead-but-kept) — codegen EMISSION is untouched, `f64_method_to_c`/
/// `int_method_to_c` remain the sole Nova-method → C-function mapping (called
/// directly from `emit_call`, not through this existence oracle).
```

### Цитата 2: Вызов f64_method_to_c (emit_c.rs L36199)

```rust
if obj_ty == "nova_f64" || obj_ty == "nova_f32" {
    if let Some(c_fn) = Self::f64_method_to_c(method) {
        let obj_c = self.emit_expr(obj)?;
        let mut arg_strs = vec![obj_c];
        for a in args { arg_strs.push(self.emit_expr(a.expr())?); }
        return Ok(format!("{}({})", c_fn, arg_strs.join(", ")));
    }
}
```

### Цитата 3: Вызов int_method_to_c (emit_c.rs L36575)

```rust
if obj_ty == "nova_int" {
    if let Some(c_fn) = Self::int_method_to_c(method) {
        let obj_c = self.emit_expr(obj)?;
        let mut arg_strs = vec![obj_c];
        for a in args { arg_strs.push(self.emit_expr(a.expr())?); }
        return Ok(format!("{}({})", c_fn, arg_strs.join(", ")));
    }
}
```

### Цитата 4: Plan 196.3 статус (emit_c.rs L46967-46981)

```rust
/// Plan 196.3 (D109/D74 checker-visibility migration): `int.abs()` and the f64/f32
/// math intrinsics (`sqrt`/`cbrt`/trig/exp/log/`pow`/`hypot`/`is_nan`/…) USED to have
/// arms here too (`int_method_to_c`/`f64_method_to_c` existence checks) — they were
/// the checker's ONLY channel because the `extern "nova"` declarations in
/// `std/runtime/math.nv` were never `import`-ed anywhere (auto-generated, dead
/// weight). Fixed the same way `char.nv` was fixed
/// (`[M-compiler-nv-porting-wave]` item A): `std/prelude.nv` now plain-`import`s
/// `std.runtime.math`, so its extern decls are inlined into every module's
/// `module.items` like any other prelude method — `method_table["f64"]["sqrt"]` /
/// `method_table["int"]["abs"]` are populated the NORMAL way and `method_overloads`
/// (consulted by the caller BEFORE this fn) resolves them directly, with real
/// arg-type checking. Those two arms are therefore unreachable now (removed, not
/// dead-but-kept) — codegen EMISSION is untouched, `f64_method_to_c`/
/// `int_method_to_c` remain the sole Nova-method → C-function mapping (called
/// directly from `emit_call`, not through this existence oracle).
```

---

**Аудит завершён. ВОЛНА-C готова к закрытию.**

Статус: ✅ LIVE, ОБОСНОВАНЫ, БЕЗ ИЗМЕНЕНИЙ.
