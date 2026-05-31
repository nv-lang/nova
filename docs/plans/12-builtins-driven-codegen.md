# План 12 — builtins.nv-driven external dispatch

**Статус:** ✅ ЗАКРЫТ (2026-05-08, кроме Ф.6 — отложен).
Ф.1-Ф.5 + Ф.7 acceptance готовы. Ф.6 (type-checker gate для unknown
methods на opaque types) — отдельный refactor types/mod.rs, не блокер
для main goal'а (single source of truth — builtins.nv).

Acceptance criterion **выполнен**: добавление `WriteBuffer @write_zero(n int)`
в builtins.nv + runtime impl `Nova_WriteBuffer_method_write_zero` —
работает БЕЗ правки Rust-codegen'а. Тест в nova_tests/runtime/write_buffer.nv.

Прежний статус: ⏳ pending (готов к старту — Plan 04 закрыт 2026-05-08).
**Связь:** [D82](../../spec/decisions/08-runtime.md#d82) (extended
2026-05-08), [D73/D77](../../spec/decisions/08-runtime.md#d73-from--into-protocol-пара-с-авто-выводом)
(auto-derive From↔Into, TryFrom↔TryInto, Fail↔Result),
Q-codegen-builtins-cleanup
([open-questions.md](../../spec/open-questions.md#q-codegen-builtins-cleanup)),
[Plan 04](04-buffer-split-and-external.md) (✅ закрыт).

## Цель

Удалить из codegen'а **только** hard-coded list-of-methods для
конкретных opaque-типов (StringBuilder/WriteBuffer/ReadBuffer).
Остальные «знания» компилятора о runtime — паттерны auto-derive,
mangling, type-mapping, intrinsics для операторов — **остаются**.

После плана компилятор **не знает наизусть**, какие методы есть у
StringBuilder; он узнаёт это, читая `std/runtime/builtins.nv`. Но
он по-прежнему **знает паттерн** «если есть `@read_X() Fail[E] -> R`,
синтезируй `@try_read_X() -> Result[R, E]`», без участия builtins.nv.

Расхождение между .nv-декларацией и runtime-реализацией компилятор
ловит **сам**, до запуска C-toolchain'а — Nova-side compile error
с понятной диагностикой (см. Ф.6 + D82 Diagnostics). Mangled
C-имена в `undefined reference` от `cc`/`clang` пользователь
никогда видеть не должен.

### Что остаётся в компиляторе

Эти знания **не вытаскиваются** в builtins.nv — они описывают
правила, не данные:

1. **Auto-derive patterns.** Компилятор знает паттерны:
   - `read_X` (Fail-form) → `try_read_X` (Result-form), D77.
   - `write_X` (Fail-form, если когда-то появится) → `try_write_X`.
   - `T.from(s S)` → `s.into() -> T`, D73 From→Into.
   - `T.from(s S) Fail[E]` → `T.try_from(s) -> Result[Self, E]`, D77.
   Источник для синтеза находится в builtins.nv (есть `read_X`?), но
   **правило синтеза** — в Rust-коде компилятора.
2. **Mangling rules.** `Nova_<T>_method_<X>` / `Nova_<T>_static_<X>`,
   plus parameter-type mangling для overload (Plan 11 Ф.3).
3. **Nova→C type mapping.** `int → nova_int`, `str → nova_str`,
   `byte → uint8_t`, `u32 → uint32_t`, `mut T → Nova_T*`, etc. Это
   правила, не таблица функций.
4. **Operator/intrinsic implementations.** `s1 + s2 → nova_str_concat(s1, s2)`,
   арифметика `+`/`-`/`*`/`/` для int/float, `==` для типов. Эти
   операторы не объявляются в builtins.nv — компилятор знает их по
   синтаксису, не по имени.

### Что вытаскивается в builtins.nv

Только **имена и сигнатуры** конкретных runtime-функций для
opaque-типов:

```nova
export external fn StringBuilder mut @append(s str) -> ()
export external fn ReadBuffer mut @read_byte() Fail[ReadBufferError] -> byte
```

«Сигнатура» здесь — **полный contract вызова**: имя, receiver-type +
`mut`-флаг, параметры (имена + типы), **return-type**, effects
(`Fail[E]`). Все компоненты участвуют в C-prototype'е и проверяются
**Nova-компилятором** против собственного внутреннего реестра
runtime-функций — return-type **тоже**: если в builtins.nv `... -> u32`,
а компилятор знает что runtime возвращает `uint64_t` — Nova-error
«signature mismatch». См.
[D82 Validation + Diagnostics](../../spec/decisions/08-runtime.md#d82)
для полной таблицы.

Из этого компилятор выводит:
- C-prototype в сгенерированный header (mangling + type-mapping всех
  компонентов сигнатуры, включая return-type) — но только после
  внутренней валидации.
- Auto-derived формы (`@try_read_byte` без явной декларации) —
  return-type Fail-формы (`-> R` под `Fail[E]`) определяет
  return-type synthesized обёртки (`-> Result[R, E]`).

### Ключевой принцип: только non-derivable формы в builtins.nv

**В builtins.nv объявляются только те external функции, которые
компилятор не может вывести автоматически по своим встроенным
паттернам.** Auto-derived формы из builtins.nv **удаляются** —
они синтезируются codegen'ом.

Сейчас в builtins.nv дублирование:
```nova
export external fn ReadBuffer mut @read_byte()       Fail[ReadBufferError] -> byte
export external fn ReadBuffer mut @try_read_byte()                          -> Result[byte, ReadBufferError]
```

После Plan 12: остаётся только Fail-форма, `try_read_byte` синтезируется
codegen'ом по встроенному паттерну. Runtime реализует **одну**
C-функцию `Nova_ReadBuffer_method_read_byte`; обёртка
`try_read_byte` — Nova-уровневая, генерируется компилятором.

**Что считается auto-derivable (паттерны компилятора):**

| Из | Авто-выводится | Правило |
|---|---|---|
| `T.from(s S) Fail[E] -> Self` | `T.try_from(s S) -> Result[Self, E]` | D77 Fail↔Result |
| `T.from(s S) -> Self` | `s.into() -> T` | D73 From→Into |
| `T mut @read_X() Fail[E] -> X` | `T mut @try_read_X() -> Result[X, E]` | D77 + naming pattern `read_*`/`try_read_*` |
| `t.into() -> T` | `T.from(t)` (зеркально) | D73 |

Auto-derived функции **наследуют видимость источника** (D5/D47):
если source `export external` — derived публичная и попадает в
prelude через D26 (для типов из `std.runtime.builtins`).

## Не цели

- **Mangling/type-mapping rules** — не трогаем. Это правила
  компилятора, не данные.
- **Operator/intrinsic implementations** (s1+s2, math, ==, etc.) —
  не трогаем. Они не external-функции, компилятор знает их по
  синтаксису.
- **Auto-derive patterns в Rust-коде** — не трогаем; они и должны
  быть в компиляторе (Plan 12 их **использует**, а не «удаляет
  таблицу с ними»).
- **Поддержка `external fn` за пределами `std.runtime.*`** — D82
  whitelist сохраняется.
- **Удаление Buffer** — это было Plan 04 Этап 6, ✅ закрыт до Plan 12.

## Текущее состояние (2026-05-08)

В `compiler-codegen/src/codegen/emit_c.rs` (классифицировано по
тому, что план **трогает**, а что нет):

**Списки методов (план Ф.5 удаляет):**

| Что | Локация |
|---|---|
| `record_schemas.insert("StringBuilder", ...)` (empty schema, opaque) | строки 411-413 |
| `record_schemas.insert("WriteBuffer", ...)` | 411-413 |
| `record_schemas.insert("ReadBuffer", ...)` | 411-413 |
| Method dispatch: `StringBuilder` (len/capacity/clone/into/append) | 4850-4877 |
| Method dispatch: `WriteBuffer` (len/capacity/clone/into/write_*) | 4879-4900 |
| Method dispatch: `ReadBuffer` (position/remaining/.../read_*/try_read_*) | 4902-4928 |
| Static-форма (`Type.factory(...)`) для всех трёх типов | 4948-5023 |

**Правила (план НЕ трогает, использует):**

| Что | Локация |
|---|---|
| Mangling instance: `format!("Nova_{}_method_{}", ...)` | 548, 658, 2830-2831 |
| Mangling static: `format!("Nova_{}_static_{}", ...)` | 5208 и др. |
| Type mapping `type_ref_to_c` | 886-985 |
| Auto-derive (Plan 08 Ф.3 для D73 From→Into) | разные места |
| Operator/intrinsic emit (s+s, math) | разные места |

**Что игнорируется codegen'ом (план Ф.1 начинает использовать):**

| Что | Локация |
|---|---|
| Парсинг builtins.nv | парсится, но `FnBody::External => {}` игнорируется codegen'ом (строки 1935, 2987, 3218) |

## Архитектура целевого решения

```
            ┌──────────────────────────┐
            │ std/runtime/builtins.nv  │  ← single source of truth
            └────────────┬─────────────┘
                         │ parse (existing parser)
                         ▼
            ┌──────────────────────────┐
            │ AST: ExternalFnDecl[]    │  ← name, receiver, params, return, effects
            └────────────┬─────────────┘
                         │ codegen passes:
                         ▼
            ┌──────────────────────────┐
            │ ExternalRegistry         │  ← keyed by (recv_type, fn_name, arity_kind)
            │  — name + C-name         │
            │  — params: Vec<C-type>   │
            │  — return: C-type        │
            │  — receiver: by-ptr/none │
            └────────────┬─────────────┘
                         │ used at:
              ┌──────────┴──────────┐
              ▼                     ▼
    emit C-prototype     emit_call resolves
    в header             dispatch через registry
                         (вместо hard-coded match)
```

`ExternalRegistry` строится **один раз** при загрузке модуля
`std.runtime.builtins`. После — только читается.

## Этапы

### Ф.1 — Сбор external деклараций из AST (~1-2ч)

1. Добавить `external_decls: Vec<ExternalFnDecl>` в codegen-state
   (или метод `collect_external_fns(&Module) -> Vec<...>`).
2. Walk module AST. Для каждого `FnDecl` где `body == FnBody::External`
   и `is_export == true` (D82 whitelist уже валидирован types/mod.rs):
   - Извлечь имя, receiver-type (если есть), `is_mut_receiver`,
     params (имена + типы), return-type, effects (для
     `Fail[ReadBufferError]`).
   - Сложить в `external_decls`.
3. Для каждой декларации применить mangling rules → `c_name: String`:
   - static: `Nova_<RecvType>_static_<fn_name>`
   - instance: `Nova_<RecvType>_method_<fn_name>`
   - free: `Nova_<module_path>_<fn_name>` (для `str.from(char)` —
     спец-case, см. Ф.4)
4. Sanity check: дубликаты mangled name? Сейчас разрешено через
   overload registry (Plan 11), нужно использовать
   parameter-type-mangling extension (Plan 11 Ф.3) при коллизии.

**Тест:** загрузить builtins.nv, распечатать registry, сравнить с
hand-rolled таблицей в emit_c.rs (текущей). Должно совпадать 1:1
после миграции — это проверка корректности парсинга.

### Ф.2 — Применить registry для record_schemas (~30мин)

В `emit_module` (строки 411-413):
```rust
// БЫЛО:
record_schemas.insert("StringBuilder".to_string(), HashMap::new());
record_schemas.insert("WriteBuffer".to_string(), HashMap::new());
record_schemas.insert("ReadBuffer".to_string(), HashMap::new());

// СТАЛО:
for decl in &external_decls {
    if let Some(recv_ty) = &decl.receiver_type {
        record_schemas.entry(recv_ty.clone())
            .or_insert_with(HashMap::new);  // opaque, schema empty
    }
}
```

Опаковые типы (StringBuilder/WriteBuffer/ReadBuffer) автоматически
регистрируются. Buffer уже удалён (Plan 04 Этап 6).

### Ф.3 — Применить registry для emit_call dispatch (~2-3ч)

Это **главный refactor**. Hard-coded match'и (строки 4850-4928,
4948-5023) заменяются на lookup в `ExternalRegistry`.

**До:**
```rust
match (recv_ty, method_name) {
    ("StringBuilder", "append") => { /* hand-rolled emit */ }
    ("StringBuilder", "len")    => { /* hand-rolled emit */ }
    ...
}
```

**После:**
```rust
if let Some(decl) = external_registry.lookup(recv_ty, method_name, &arg_types) {
    emit_external_call(&decl, &args, &mut output);
    return;
}
// fallback на user-defined record методы (без изменений)
```

`emit_external_call` — общая функция, которая на основе
`decl.params: Vec<CType>` + `decl.return_ty: CType` генерирует:
- C-syntax вызова: `Nova_X_method_y(receiver_ptr, arg0, arg1, ...)`
- Нужные приведения: `as nova_int` / `as uint32_t` etc.
- Обработку Fail-эффектов (sets `*err` параметр, как сейчас в
  ReadBuffer).

**Edge cases:**
- Overloaded методы (`StringBuilder.@append(s str)` vs `@append(c char)`):
  registry хранит обе декларации, lookup выбирает по типам args
  (Plan 11 Ф.3 mangling).
- `@into()` overloads (`StringBuilder @into() -> str` vs
  `WriteBuffer @into() -> []byte`): разные receiver'ы — нет
  коллизии.
- Static-форма (`StringBuilder.with_capacity(n)`): registry знает
  `is_static`, mangling `Nova_StringBuilder_static_with_capacity`.
- Self в return-type (`StringBuilder.new() -> Self`): уже работает
  через `current_receiver_type`; не требует изменений.
- Fail-effects: декларация несёт `effects: [Fail[ReadBufferError]]`,
  emit_external_call добавляет `*err` параметр и unwind.

### Ф.4 — Free-функции (`str.from(char)`) (~30мин)

`export external fn str.from(c char) -> Self` — это "method on `str`".
Mangling: `Nova_str_static_from`. Сейчас обрабатывается отдельным
branch в emit_call.

После Ф.3 — единый путь через registry. Receiver-type = `"str"`,
is_static = true.

**Особенность:** `str` не record, не opaque-тип, а primitive
(`nova_str` = struct в runtime). C-функция возвращает `nova_str`
по value, не `Nova_str*`. Убедиться что `type_ref_to_c("Self")` в
контексте receiver=str даёт `nova_str` корректно.

### Ф.4.5 — Auto-derive non-declared формы (~2-3ч) ❌ ОТМЕНЕНО

**Статус.** Этот этап **отменён** в Plan 13 Ф.9.5 (см.
docs/plans/13-runtime-stdlib-and-autogen.md → Ф.9.5).

**Причины отмены:**
1. **Hidden magic** — в registry / `read_buffer.nv` видна только
   Fail-форма, но автокомплит и AI-codegen видят `try_read_X` неоткуда.
2. **Edge cases** — UTF-8 ошибки в `read_char`/`read_str` (Plan 13
   Ф.9.4) делают universal правило хрупким: synthesized форма должна
   корректно мапить и `UnexpectedEnd`, и `InvalidUtf8`.
3. **D82 single source of truth** — auto-derive противоречит принципу
   «всё что компилятор умеет — видно в registry».

**Что вместо.** В `runtime_registry.rs` (Plan 13) явно перечислены
обе формы для каждого read-метода: Fail-form и Result-form. C-функции
тоже две (одна на Fail, одна на Result). См. Plan 13 Ф.9.5.

**D73 From↔Into auto-derive остаётся** — это симметричное правило
прописано в D73, не зависит от данной отмены.

---

Исходный (отменённый) текст Ф.4.5 ниже сохранён как историческая
справка:

**Цель.** Удалить из builtins.nv все формы, которые компилятор может
вывести автоматически по D73/D77, и синтезировать их в codegen'е.

**Что удаляется из builtins.nv:**

1. **`ReadBuffer.@try_read_*`** (16+ функций — `try_read_byte`,
   `try_read_u8/16/32/64_le/be`, `try_read_i*`, `try_read_f*`).
   Остаётся только `@read_*` (Fail-форма).
2. **`char.into() -> str`** — если он есть как явная декларация
   (сейчас комментарий в builtins.nv). Выводится из
   `str.from(c char)` по D73.
3. **Любые `Type.try_from(...)` пары к `Type.from(...) Fail[E]`** —
   если есть. Сейчас в builtins.nv нет, но правило фиксируется.

**Codegen-логика (D77 Fail↔Result auto-derive):**

При обходе `external_registry` для каждой Fail-form декларации
`fn T mut @read_X() Fail[E] -> X` codegen **синтезирует** Nova-AST
для `try_read_X`:

```nova
fn T mut @try_read_X() -> Result[X, E] {
    ro r = with Fail[E] = (e) => interrupt Err(e) {
        Ok(@read_X())
    }
    r
}
```

(Не C-функцию — Nova-уровневую обёртку. Runtime реализует только
одну C-функцию `Nova_T_method_read_X`.)

Эта синтезированная декларация добавляется в `external_registry`
**виртуально** (с пометкой `is_derived: true`, чтобы Ф.5 знала что
её не надо искать в builtins.nv AST). При вызове `rb.try_read_X()`
codegen эмитит inline-обёртку над `Nova_T_method_read_X`.

**Codegen-логика (D73 From→Into auto-derive):**

Уже работает в Plan 08 Ф.3 (`str.from(c char)` → `c.into() -> str`).
Plan 12 не меняет этот механизм, только подключает к
`external_registry` lookup'у — `into()` calls должны находить
synthesized декларацию через тот же registry, что и `read_*`.

**Pattern recognition для `read_*`/`try_read_*`:**

Codegen матчит по двум критериям:
1. Имя method'а начинается с `read_` (после `@`).
2. Сигнатура: `fn T mut @read_<X>(...) Fail[E] -> R`.

Если оба условия выполнены — синтезирует `try_read_<X>`. Если в
builtins.nv явно объявлена `try_read_X` — это **ошибка** (auto-
derived form не должна объявляться вручную):

```
error: external fn `T.@try_read_X` is auto-derived from `T.@read_X`
       (D77 Fail↔Result); remove the explicit declaration from
       std/runtime/builtins.nv
```

**Visibility:** synthesized функции наследуют `is_export` от source.
Все 17 `read_*` в builtins.nv — `export external`, значит все
synthesized `try_read_*` тоже public и попадают в prelude через D26.

**Тест:** удалить `@try_read_byte` из builtins.nv, прогнать
`nova_tests/runtime/read_buffer.nv` — должно работать (synthesized
обёртка эмитится автоматически). Затем добавить `@try_read_byte`
явно — компилятор должен дать error «auto-derived; remove».

### Ф.5 — Удалить hard-coded list-of-methods (~30мин)

После того как Ф.1-Ф.4.5 работают и проходят тесты:

Удалить из `emit_c.rs` **именно списки методов конкретных типов**
(не паттерны и не правила mangling):

- Method dispatch для StringBuilder (4850-4877) — это hard-coded
  список «вот такие методы есть у StringBuilder».
- Method dispatch для WriteBuffer (4879-4900).
- Method dispatch для ReadBuffer (4902-4928).
- Static-форма Buffer/StringBuilder/WriteBuffer/ReadBuffer
  (4948-5023; Buffer уже удалён в Plan 04 Этап 6).
- Hard-coded `record_schemas.insert(...)` для трёх типов
  (заменено в Ф.2 на «inserts via registry walk»).

**Что НЕ удаляется** (это правила, не данные):
- Mangling helper'ы (`format!("Nova_{}_method_{}", ...)`) — остаются,
  теперь вызываются из registry-builder, а не из emit_call.
- `type_ref_to_c` (Nova→C type mapping) — целиком остаётся.
- `emit_external_call` (общий driver вызова) — это новый код Ф.3,
  не таблица.
- Auto-derive patterns Ф.4.5 — остаются как Rust-код.
- Operator/intrinsic дескриптор (`s1+s2 → nova_str_concat`) — не
  трогаем, это другой механизм.

Diff в LoC: ожидается негативный на ~150-200 строк (только списки
методов), не больше. Ничего «универсально-полезного» не удаляется.

### Ф.6 — Diagnostics: компилятор валидирует сам, без C-toolchain'а (~2ч)

**Принцип:** Nova компилируется в C, который потом обрабатывается
`cc`/`clang`/`MSVC`. Их линкер существует, но **мы не полагаемся
на него для пользовательской диагностики** — mangled C-имя
(`undefined reference to Nova_X_method_y`) не понятно тому, кто
пишет на Nova.

Компилятор **сам** знает свой bundled runtime (компилятор и runtime
— один артефакт, версионируются вместе). Все расхождения между
builtins.nv и runtime ловятся внутри Nova-компилятора и выдаются
как Nova-error до запуска `cc`.

**Таксономия ошибок (полная — все случаи должны быть реализованы):**

| Случай | Когда ловится | Сообщение |
|---|---|---|
| User вызывает несуществующий метод opaque-типа (`sb.unknown()`) | type-check (после Ф.1 registry готов) | `no method 'unknown' on StringBuilder. Available: append, len, capacity, ...` |
| `external fn X.@y` в builtins.nv, но runtime реализации нет | при загрузке builtins.nv в codegen | `external fn 'StringBuilder.@y' not implemented in runtime. Either remove from std/runtime/builtins.nv or add the implementation to nova_rt/string_builder.c` |
| Сигнатура в builtins.nv не совпадает с реализацией компилятора (тип параметра, return-type, effects) | при загрузке builtins.nv | `signature mismatch for 'StringBuilder.@append': declared 'fn (s str) -> ()', runtime expects 'fn (s str) -> int'` |
| Codegen эмитит вызов внешней функции, не объявленной в builtins.nv | внутренний invariant | `compiler bug: emitted call to undeclared external 'X.@y'` (internal compile error; не должно случаться у пользователя) |
| User объявил auto-derived форму (`@try_read_X` рядом с `@read_X`) | при загрузке builtins.nv | `'@try_read_X' is auto-derived from '@read_X' (D77 Fail↔Result); remove from std/runtime/builtins.nv` |

**Реализация:**

1. **Внутренний реестр компилятора.** Codegen ведёт
   `runtime_implemented: HashMap<C_name, CSignature>` — какие функции
   он реально знает в bundled runtime. Источник — Rust-код в codegen,
   компилируемый вместе с компилятором (это **legitimate** hard-coded
   knowledge: компилятор и runtime — один артефакт). Plan 12 не
   удаляет его, наоборот — формализует.
2. **Validation pass** при загрузке builtins.nv: для каждой
   `external fn X.@y(...)` декларации:
   - Compute mangled C-name + expected C-signature.
   - Lookup в `runtime_implemented`. Нет → ошибка «not implemented».
   - Compare signatures. Расходятся → ошибка «signature mismatch».
3. **Type-check user code:** при resolve `sb.method(args)` для opaque
   receiver — lookup в `external_registry` (наполняется из validated
   builtins.nv). Нет → «no method». Available list берётся из
   registry для good error message.
4. **C-toolchain как safety net.** Если всё-таки `cc` выдаст
   `undefined reference` — это **bug в Nova-компиляторе** (`runtime_implemented`
   реестр был неполным или validation не сработала). Bug-report,
   не пользовательская проблема.

**Тесты в `nova_tests/errors/`:**

```nova
// (1) unknown method
test "StringBuilder unknown method" {
    mut sb = StringBuilder.new()
    sb.unknown_method()
    // expected: error: no method 'unknown_method' on StringBuilder
}

// (5) auto-derive collision
// builtins.nv добавляется явный @try_read_byte:
//   export external fn ReadBuffer mut @try_read_byte() -> Result[byte, ReadBufferError]
// expected: error: '@try_read_byte' is auto-derived...
```

Случаи 2/3/4 тестируются через unit-тесты компилятора (Rust-тесты
на validation pass), не через `nova_tests/`.

### Ф.7 — Документация и discussion-log (~30мин)

1. `docs/project-creation.txt` — описать что builtins.nv теперь
   driver, hard-coded таблицы удалены.
2. `docs/simplifications.md` — записать как simplification (1 source
   вместо 2).
3. `nova-lang-private/discussion-log.md` — резюме эволюции D82 →
   Plan 12.
4. В `compiler-codegen/README.md` (если есть) — раздел "Adding new
   external fn": один edit в builtins.nv + импл в `nova_rt/`, codegen
   не трогается.

## Тесты

- **Все существующие тесты должны продолжать проходить** (это refactor,
  не feature). 42/42 codegen pass rate сохраняется.
- **Новый negative test** (Ф.6): unknown method on opaque type —
  type-error, не linker-error.
- **Sanity test:** добавить в builtins.nv новую external fn (например
  `WriteBuffer mut @write_u128_le(v u128) -> ()`), реализовать в
  runtime — должна работать без правки emit_c.rs. Это **acceptance
  criterion** Plan'а 12.

## Зависимости

- ✅ Plan 04 (включая Этап 6 — Buffer удалён) — закрыт 2026-05-08.
- ✅ Parsing `external fn` — работает (lexer/parser/types).
- ✅ Plan 08 Ф.3 (D73 From→Into auto-derive) — закрыт; Plan 12 Ф.4.5
  расширяет тот же механизм паттерном `read_*`/`try_read_*` (D77).

## Риски

1. **Overload-collision с user-defined methods.** Если кто-то напишет
   `fn StringBuilder mut @custom() -> ()` (не external) — должен ли
   это работать? D26 говорит StringBuilder — built-in opaque, нельзя
   расширять методами в user-коде. Type-checker должен запретить
   `fn <opaque-type> @<method>` за пределами `std.runtime.*`.
   Проверить, что это уже валидируется (если нет — добавить).
2. **Bootstrap circular.** Codegen зависит от builtins.nv → builtins.nv
   парсится codegen'ом. Но: парсинг — это lexer+parser+types, не
   codegen. Codegen только **читает** AST, что приходит из types.
   Цикла нет; порядок: types module процессит builtins.nv первым (он
   зависит только от prelude), затем все остальные модули видят
   external_registry.
3. **Performance.** Lookup в HashMap на каждый method call vs
   hard-coded match. Insignificant: registry небольшая (~70 entries),
   match'и тоже линейные. Не оптимизируем.

## Acceptance criteria

- [ ] Ф.1: registry строится из builtins.nv AST, печатается, совпадает
      с hand-rolled таблицей.
- [ ] Ф.2: record_schemas наполняется из registry.
- [ ] Ф.3: emit_call использует registry для StringBuilder/WriteBuffer/
      ReadBuffer; все 42/42 codegen теста проходят.
- [ ] Ф.4: `str.from(char)` работает через registry.
- [ ] Ф.4.5: `try_read_*` удалены из builtins.nv (≥17 деклараций),
      codegen синтезирует обёртки автоматически; runtime тесты
      `read_buffer.nv` проходят без изменений; явная декларация
      auto-derived формы — compile error.
- [ ] Ф.5: hard-coded таблицы удалены из emit_c.rs (4850-5023);
      diff в LoC негативный на ~200 строк.
- [ ] Ф.6: type-checker отвергает unknown method на opaque-типе.
- [ ] Ф.7: docs обновлены.
- [ ] **Sanity 1:** добавление новой Fail-form `external fn` в
      builtins.nv + runtime-impl работает без правки Rust-codegen'а;
      `try_*` форма доступна автоматически.
- [ ] **Sanity 2:** добавление новой `T.from(s S) Fail[E]` в
      builtins.nv даёт `T.try_from(s)` и `s.into()` бесплатно
      (через D73/D77 + Plan 12 Ф.4.5).

## Open questions внутри плана

1. **Effect signatures в registry.** `Fail[ReadBufferError]` нужно
   сохранять как часть декларации, чтобы emit_external_call знал что
   добавить `*err` параметр. Простая реализация: `effects: Vec<EffectId>`
   в `ExternalFnDecl`. Уже есть в AST.
2. **Generic external fns.** Сейчас не нужны (все external —
   monomorphic). Если когда-то понадобится `external fn collect[T]()`
   — refactor. Не делаем.
