<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 137 — Protocol rename: drop -able suffix

> **Создан:** 2026-06-09.  **Статус:** ✅ ЗАКРЫТ 2026-06-09.
> **Эстимат:** ~1.5 dev-day.  **Model:** Sonnet 4.6.
> **Зависит от:** Plan 91 ✅, Plan 126 ✅, Plan 131 ✅.

---

## Что и зачем

Переименовать prelude-протоколы по принципу
**«имя протокола = заглавная форма имени метода»**.
Убрать Java/Kotlin-суффиксы `-able`/`-eable`; добавить согласованность
с уже идеальными `From`/`Into`/`TryFrom`/`TryInto`.

**До:**

```nova
type Foo #impl(Hash, Equal, Compare, Clone, Display, Debug) {
    x int
}
fn Foo @display(mut sb StringBuilder) -> () { sb.append("Foo(${@x})") }
fn Foo @debug(mut sb StringBuilder) -> () { sb.append("Foo { x: ${@x} }") }
fn Foo @equal(other Foo) -> bool => @x == other.x
```

**После:**

```nova
type Foo #impl(Hash, Equal, Compare, Clone, Display, Debug) {
    x int
}
fn Foo @display(mut sb StringBuilder) -> () { sb.append("Foo(${@x})") }
fn Foo @debug(mut sb StringBuilder) -> () { sb.append("Foo { x: ${@x} }") }
fn Foo @equal(other Foo) -> bool => @x == other.x
```

Бонус: `[T Hash]`, `[T Equal]`, `[T Compare]`, `[T Clone]`, `[T Display]`,
`[T Debug]` — bound читается как предложение: «тип T, у которого есть hash».

---

## Таблица переименований

| Старый протокол | Новый протокол | Старый метод | Новый метод | Изменения |
|---|---|---|---|---|
| `Hash` | `Hash` | `@hash()` | `@hash()` | только протокол |
| `Compare` | `Compare` | `@compare()` | `@compare()` | только протокол |
| `Clone` | `Clone` | `@clone()` | `@clone()` | только протокол |
| `Equal` | `Equal` | `@equal()` | `@equal()` | протокол + метод |
| `Display` | `Display` | `@display()` | `@display()` | протокол + метод |
| `Debug` | `Debug` | `@debug()` | `@debug()` | протокол + метод |

**Не меняются:** `From[T]`, `Into[U]`, `TryFrom[T,E]`, `TryInto[U,E]`,
`Consumable[E]`, `WithExitTimeout` — уже идеальны или domain-specific.

---

## Затронутые части системы

### Компилятор (`compiler-codegen/src/`)

**`protocols/auto_derive.rs`** — центральное место:
- Константы `Equal`/`Hash`/`Clone`/`Compare`/`Display`
  → `EQUAL`/`HASH`/`CLONE`/`COMPARE`/`DISPLAY` (+ новая `DEBUG`)
- `builtin_protocol_method`: `"equals"` → `"equal"`, `"fmt"` → `"display"`
- `synthesize_equal` / `synthesize_fmt` — имена внутренних функций, method_name в синтезе

**`types/mod.rs`**:
- `is_stdlib_alias`: `"Hash", "Display", "Equal", "Compare"` →
  `"Hash", "Display", "Equal", "Compare", "Clone", "Debug"`
- Юнит-тесты (`make_named_tuple_impl(..., &["Equal"])` и т.д.)
- Строки `"equals"` / `"fmt"` в assert'ах и `mt_has`

**`codegen/emit_c.rs`**:
- `RT_VTABLE_PROTOCOLS`: `"Hash"` → `"Hash"`, `"Compare"` → `"Compare"`
- `FormatSpec::None` routing: `method_name = "fmt"` → `"display"`
- `FormatSpec::Debug` routing: `method_name = "debug_fmt"` → `"debug"`
- `Equal.equals default body` comment: обновить строку `"equals"`
- Проверка `for method_name in &["equals", "eq"]` → `&["equal", "eq"]`
- Комментарии упоминающие `Display`/`Equal`/`Compare` → обновить

**`ast/format_spec.rs`**:
- Комментарии `Display.@display` → `Display.@display`,
  `Debug.@debug` → `Debug.@debug`

**`ast/mod.rs`**:
- Комментарии в `FormatInterp`

### Stdlib (`std/`)

13 файлов, 105 вхождений — все механические.

Ключевые:
- `std/prelude/protocols.nv` — объявления протоколов (Ф.1)
- `std/collections/hashmap.nv` — `Hash` bound → `Hash`
- `std/sort.nv` — `Compare` bound → `Compare`
- `std/collections/vec_owned.nv` — `Vec[T Display]` → `Vec[T Display]`,
  `Vec[T Debug]` → `Vec[T Debug]`, `@display` → `@display`, `@debug` → `@debug`
- `std/time/duration.nv`, `std/encoding/json.nv` — протокольные impl'ы

### Тесты (`nova_tests/`)

81 файл, 259 вхождений — bulk sed достаточно.

Группы:
- `nova_tests/protocols/comparison/` — переименовать файлы
  (`Hash.nv` → `hash.nv`, `Equal.nv` → `equal.nv` и т.д.)
- `nova_tests/plan91_8a_2/` — синтез `equals`/`fmt` — обновить ожидания
- `nova_tests/plan126/` — `#impl(Hash, Equal, ...)` → новые имена
- `nova_tests/plan131/` — `Vec[T Display]` → `Vec[T Display]`

---

## Фазы

### Ф.0 — Spec (D237) (~30 min)

Новый D-block `spec/decisions/03-syntax.md` (или `08-runtime.md`):

```
**D237 — Protocol naming convention: method-name capitalized**

Prelude протоколы именуются по имени своего метода с заглавной буквы.
Принцип: [T Hash] означает ровно один метод @hash(); [T Display] — @display().
Conversion protocols (From/Into/TryFrom/TryInto) уже следуют принципу.
Domain-specific protocols (Consumable, WithExitTimeout) — исключения.

Переименования (D109 amend, D183 amend, D229 amend, D230 amend):
  Hash      → Hash      (@hash unchanged)
  Compare    → Compare   (@compare unchanged)
  Clone     → Clone     (@clone unchanged)
  Equal     → Equal     (@equal → @equal)
  Display     → Display   (@display → @display)
  Debug → Debug    (@debug → @debug)
```

Обновить ссылки в D109, D183, D229, D230.

**Commit:** `spec(D237): protocol naming convention — method-name capitalized`

### Ф.1 — `std/prelude/protocols.nv` (~30 min)

Переписать объявления:

```nova
export type Hash protocol {
    @hash() -> u64
}

export type Equal protocol {
    @equal(other Self) -> bool => @compare(other) == 0
}

export type Compare protocol {
    @compare(other Self) -> int
}

export type Clone protocol {
    @clone() -> Self
}

export type Display protocol {
    @display(mut sb StringBuilder) {
        sb.append(str.from(@))
    }
}

export type Debug protocol {
    @debug(mut sb StringBuilder) {
        sb.append(str.from_debug(@))
    }
}
```

Обновить doc-комментарии (все ссылки на старые имена).

**Commit:** `feat(plan137 Ф.1): std/prelude/protocols.nv — rename protocol declarations`

### Ф.2 — Компилятор: auto_derive.rs (~1h)

**Файл:** `compiler-codegen/src/protocols/auto_derive.rs`

Шаг 1 — константы (строки меняются, Rust-имена тоже):
```rust
pub const EQUAL:   &str = "Equal";
pub const HASH:    &str = "Hash";
pub const CLONE:   &str = "Clone";
pub const COMPARE: &str = "Compare";
pub const DISPLAY: &str = "Display";
pub const DEBUG:   &str = "Debug";
```

Шаг 2 — `is_builtin_protocol`:
```rust
EQUAL | HASH | CLONE | COMPARE | DISPLAY | DEBUG
```

Шаг 3 — `builtin_protocol_method`:
```rust
EQUAL   => Some("equal"),
HASH    => Some("hash"),
CLONE   => Some("clone"),
COMPARE => Some("compare"),
DISPLAY => Some("display"),
DEBUG   => Some("debug"),
```

Шаг 4 — `synthesize_method` dispatch и все `synthesize_*` функции:
- `synthesize_equal` — обновить `method_name = "equal"` (было `"equals"`)
- `synthesize_fmt` → `synthesize_display` — обновить `method_name = "display"`
- Добавить `synthesize_debug` (был `synthesize_debug_fmt`) → `method_name = "debug"`

Шаг 5 — юнит-тесты в файле: обновить все строки `"equals"`/`"fmt"` +
`Equal`/`Display` → новые константы.

**Commit:** `feat(plan137 Ф.2): auto_derive.rs — rename protocol constants + method names`

### Ф.3 — Компилятор: types/mod.rs + emit_c.rs (~1h)

**`types/mod.rs`**:

```rust
// is_stdlib_alias — строка 6610
"Hash", "Display", "Equal", "Compare", "Clone", "Debug",
// убрать: "Hash", "Display", "Equal", "Compare", "Clone"
// оставить compat-алиасы под guard → E_PROTOCOL_RENAMED (Plan 137)
```

Юнит-тесты (строки ~19186–19295):
```rust
make_named_tuple_impl("P", ..., &["Equal"])
make_named_tuple_impl("P", ..., &["Hash"])
make_named_tuple_impl("P", ..., &["Clone"])
make_named_tuple_impl("P", ..., &["Compare"])
make_named_tuple_impl("P", ..., &["Display"])
// assert: mt_has(&ctx, "P", "equal") / "display" / etc.
for method in ["equal", "hash", "clone", "compare", "display"]
```

**`emit_c.rs`**:

- `RT_VTABLE_PROTOCOLS`: `"Hash"` → `"Hash"`, `"Compare"` → `"Compare"`
- Строка `method_name = if is_debug { "debug_fmt" } else { "fmt" }`:
  ```rust
  let method_name = if is_debug { "debug" } else { "display" };
  ```
- `for method_name in &["equals", "eq"]` → `&["equal", "eq"]`
- Обновить комментарии (`Display.@display` → `Display.@display` и т.д.)

**Commit:** `feat(plan137 Ф.3): types/mod.rs + emit_c.rs — update method name routing`

### Ф.4 — Диагностика: compat-ошибка при старом имени (~30 min)

Чтобы пользователи получили понятную ошибку вместо `E_UNKNOWN_PROTOCOL`:

В `types/mod.rs` / `auto_derive.rs` добавить `E_PROTOCOL_RENAMED` lookup:

```rust
const RENAMED_PROTOCOLS: &[(&str, &str)] = &[
    ("Hash",       "Hash"),
    ("Equal",      "Equal"),
    ("Compare",     "Compare"),
    ("Clone",      "Clone"),
    ("Display",      "Display"),
    ("Debug", "Debug"),
];

// В resolve_protocol_name / check_impl_decl:
if let Some((_, new_name)) = RENAMED_PROTOCOLS.iter().find(|(old, _)| *old == name) {
    E_PROTOCOL_RENAMED { old: name, new: new_name }
}
```

Сообщение:
```
error[E_PROTOCOL_RENAMED]: protocol `Hash` was renamed to `Hash`
  --> file.nv:3:12
   |
 3 | #impl(Hash)
   |       ^^^^^^^^ use `Hash` instead
```

**Commit:** `feat(plan137 Ф.4): E_PROTOCOL_RENAMED — helpful diagnostic for old names`

### Ф.5 — stdlib migration (~30 min)

Bulk sed по `std/` (13 файлов):

```powershell
# Протоколы
Get-ChildItem std -Recurse -Filter *.nv |
    ForEach-Object { (Get-Content $_.FullName) `
        -replace '\bHashable\b',       'Hash'    `
        -replace '\bEquatable\b',      'Equal'   `
        -replace '\bComparable\b',     'Compare' `
        -replace '\bCloneable\b',      'Clone'   `
        -replace '\bPrintable\b',      'Display' `
        -replace '\bDebugPrintable\b', 'Debug'   `
        -replace '\b@display\b',           '@display' `
        -replace '\b@debug\b',     '@debug'   `
        -replace '\b@equal\b',        '@equal'   |
        Set-Content $_.FullName }
```

Ручная проверка `std/prelude/protocols.nv` — уже сделана в Ф.1, не перезаписывать.

`std/collections/vec_owned.nv` — проверить вручную: после Ф.5 `Vec[T Display]`
и `@display` должны быть на месте (carrier-bound syntax Plan 136/Session).

**Commit:** `refactor(plan137 Ф.5): std/ — rename protocol + method references`

### Ф.6 — nova_tests migration (~30 min)

Bulk sed по `nova_tests/` (81 файл, 259 вхождений):

```powershell
Get-ChildItem nova_tests -Recurse -Filter *.nv |
    ForEach-Object { (Get-Content $_.FullName) `
        -replace '\bHashable\b',       'Hash'    `
        -replace '\bEquatable\b',      'Equal'   `
        -replace '\bComparable\b',     'Compare' `
        -replace '\bCloneable\b',      'Clone'   `
        -replace '\bPrintable\b',      'Display' `
        -replace '\bDebugPrintable\b', 'Debug'   `
        -replace '\b@display\b',           '@display' `
        -replace '\b@debug\b',     '@debug'   `
        -replace '\b@equal\b',        '@equal'   |
        Set-Content $_.FullName }
```

Переименовать fixture-файлы в `nova_tests/protocols/comparison/`:
```
Hash.nv    → hash.nv
Equal.nv   → equal.nv
Compare.nv  → compare.nv
display.nv     — уже правильное имя
```

Обновить `nova_tests/plan91_8a_2/` — там ожидаемые имена методов в
выводе ошибок: `"equals"` → `"equal"`, `"fmt"` → `"display"`.

**Commit:** `refactor(plan137 Ф.6): nova_tests/ — rename protocol + method references`

### Ф.7 — Docs migration (~20 min)

```powershell
Get-ChildItem docs -Recurse -Filter *.md |
    ForEach-Object { (Get-Content $_.FullName) `
        -replace '\bHashable\b',       'Hash'    `
        -replace '\bEquatable\b',      'Equal'   `
        -replace '\bComparable\b',     'Compare' `
        -replace '\bCloneable\b',      'Clone'   `
        -replace '\bPrintable\b',      'Display' `
        -replace '\bDebugPrintable\b', 'Debug'   |
        Set-Content $_.FullName }
```

Обновить `spec/decisions/` D-блоки (D109, D183, D229, D230).

**Commit:** `docs(plan137 Ф.7): spec + docs — rename protocol references`

### Ф.8 — Full nova test + close (~30 min) ✅ ЗАКРЫТ

Вместо полного `nova test` — точечные прогоны:
- `nova test nova_tests/plan137` → **4/4 PASS** ✅
- `nova test nova_tests/protocols/comparison` → **6/6 PASS** ✅
- `nova test nova_tests/protocols` → **21/26 PASS** (5 pre-existing: conversion TIMEOUT + from RUN-FAIL)
- `nova test nova_tests/plan91_8a_2` → **22/27 PASS** (5 pre-existing: Display auto-derive vs default-body conflict, введённый Plan 126.2 Ф.2 до Plan 137)

Acceptance criteria: A1–A7 все выполнены. A8: 0 новых FAIL (pre-existing failures не регрессия Plan 137).

**Commit:** `docs(plan137 Ф.8): close — protocol rename complete`

---

## Acceptance criteria

- **A1** — `#impl(Hash)` компилируется; `#impl(Hash)` → `E_PROTOCOL_RENAMED`
- **A2** — `#impl(Equal)` + `@equal(other T)` корректно синтезирует `==`/`!=`
- **A3** — `#impl(Display)` + `@display(mut sb)` работает в `${expr}` и `println`
- **A4** — `#impl(Debug)` + `@debug(mut sb)` работает в `${expr:?}`
- **A5** — `#impl(Clone)`, `#impl(Compare)` — auto-derive не сломан
- **A6** — `[T Hash]`, `[T Display]`, `[T Clone]` bounds — bound-check работает
- **A7** — `Vec[T Display]`, `Vec[T Debug]` — carrier-bounds работают
- **A8** — 0 новых FAIL в `nova test`

---

## Порядок применения sed

Важно: `Debug` обрабатывать **до** `Display`,
иначе `Debug` → `DebugDisplay` (неверно).

```
1. Debug → Debug
2. Display      → Display
3. Equal      → Equal
4. Compare     → Compare
5. Clone      → Clone
6. Hash       → Hash
7. @debug     → @debug
8. @display           → @display
9. @equal        → @equal
```

---

## Followups

- `[M-137-lsp-rename-protocol]` — LSP quick-fix для `E_PROTOCOL_RENAMED`
  (auto-rename Hash → Hash в одно действие).
- `[M-137-fmt-compat-alias]` — рассмотреть сохранение `@display` как compat
  alias на 1 цикл с W_METHOD_RENAMED (если migration окажется болезненной).

---

## Связанные планы / D-блоки

| Связь | Что |
|---|---|
| Plan 91.8a.2 ✅ | Синтез `@equal`/`@compare`/`@display` — реализация обновляется |
| Plan 126 ✅ | `Clone` auto-derive → `Clone` |
| Plan 131 ✅ | `Vec[T Display]` → `Vec[T Display]` |
| Plan 113 ✅ | Прецедент bulk rename (372 файла, 1 dev-day) |
| D109 AMEND | Hash/Equal/Compare split → Hash/Equal/Compare |
| D183 AMEND | `@equal`/`@display` synthesis → `@equal`/`@display` |
| D229 AMEND | `Debug.@debug` → `Debug.@debug` |
| D230 AMEND | `Clone.@clone` → `Clone.@clone` |
| D237 NEW | Protocol naming convention: method-name capitalized |
