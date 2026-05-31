# Plan 107 — Prelude attribute syntax migration

> **Статус:** ✅ CLOSED 2026-05-27
> **Приоритет:** P1 — quality/consistency (не блокирует фичи, но: неконсистентный
>   синтаксис, сломанный `_module.nv`-inheritance, production-grade technical debt)
> **Оценка:** ~1 dev-day (выполнено за ~1 day)
> **Зависимости:** Plan 62.F ✅, Plan 62.F.bis ✅, D26 ✅, D100 ✅
> **D-решение:** D174 (07-modules.md) — amends D78 §«Prelude opt-out»,
>   Plan 62.F, Plan 62.F.bis; extends D100 `_module.nv`
>
> **Recommended model:** Sonnet 4.6, Thinking OFF (механическая миграция).
>
> **Результат:** Commits `0adc9e8ceba` + `0229f4d1d61` · Branch `plan-107` merged в main.
> 11 тестов plan107/ (8 PASS + 3 CC-FAIL pre-existing) · 0 регрессий · 1451/62 full suite.
> Исправлен pre-existing bug: `#no_prelude` в `_module.nv` не работал для peers (Ф.3).

---

## Контекст и мотивация

D99 (Plan 42.16) закрепил правило: **module-level атрибуты идут ПЕРЕД `module`
declaration** — консистентно с `#cfg`/`#doc`/`#forbid` перед `fn`. Тем не менее
Plan 62.F/62.F.bis ввёл три inline-клаузы как исключение:

```nova
// БЫЛО (Plan 62.F / 62.F.bis):
module collections.range partial_prelude(core, runtime, errors) allow_prelude_shadow
```

Это нарушение установленного паттерна плюс ряд production-grade недостатков:

| Проблема | Последствие |
|---|---|
| `partial_` в `partial_prelude` — лишнее слово | 19+ символов лишнего boilerplate |
| Inline на module-line нарушает D99 паттерн | Inconsistency с `#cfg`/`#doc`/`#forbid` |
| `partial_prelude()` == `no_prelude` — два синтаксиса одного смысла | Footgun |
| `allow_prelude_shadow` — 19 символов | Нечитаемо в комбинации |
| Inline-клаузы не наследуются через `_module.nv` | `#no_prelude` в `_module.nv` не работает |
| В `imports.rs` prelude-check происходит ДО merge `inherited_attrs` | Pre-existing bug: `_module.nv` prelude opt-out сломан |

### Новый синтаксис (D174)

```nova
// СТАЛО:
#no_prelude
module my.realtime

#prelude(core, runtime, errors)
#allow(shadow)
module collections.range
```

**Семантика атрибутов:**

| Атрибут | Соответствие в old syntax | Семантика |
|---|---|---|
| `#no_prelude` | `no_prelude` clause | Полный opt-out |
| `#prelude(names…)` | `partial_prelude(names…)` clause | Selective opt-in; ≥1 имя |
| `#allow(shadow)` | `allow_prelude_shadow` clause | Suppress W_PRELUDE_SHADOW |

**Изменение поведения:**
- `#prelude()` (пустой список) → **compile error**: `"use #no_prelude for empty prelude"`.
  Было: `partial_prelude()` молча трактовалось как `no_prelude`.
- Старые inline-формы → **compile error** с migration hint (не deprecated warning).
  Nova pre-production → жёсткая миграция без grace period.

---

## Worktree setup

**Convention:** постоянный worktree `nova-p107` на ветке `plan-107`.

```bash
git worktree add -b plan-107 ../nova-p107 main
cd ../nova-p107
```

Для `nova test` в worktree — env vars на main:

```bash
export NOVA_GC_LIB_DIR=d:/Sources/nv-lang/nova/compiler-codegen/vcpkg_installed/x64-windows-static/lib
export NOVA_GC_INCLUDE_DIR=d:/Sources/nv-lang/nova/compiler-codegen/vcpkg_installed/x64-windows-static/include
```

---

## Model / Thinking mode per phase

| Phase | Thinking | Why |
|---|---|---|
| Ф.0 Spec | OFF | механическое написание по шаблону |
| Ф.1 Parser new forms | OFF | добавление в `parse_module_attrs()` по образцу |
| Ф.2 Parser remove old forms | OFF | замена на `return Err(...)` с hint |
| Ф.3 imports.rs prelude-inheritance fix | **ON** | restructuring порядка операций |
| Ф.4 validate empty `#prelude()` | OFF | одна проверка в parser |
| Ф.5 Lint message update | OFF | string replace |
| Ф.6 Fixture + std/ migration | OFF | mechanical renames |
| Ф.7 Tests | OFF | explicit list |
| Ф.8 Close | OFF | trivial |

---

## Фазы

### Ф.0 — Spec: D174 + amend D78, D100, README

**Файлы:**
- `spec/decisions/07-modules.md`
- `spec/decisions/README.md`

#### 07-modules.md

Три места:

**1. D78 §«Prelude opt-out» (строки ~1102-1216)** — заменить весь раздел согласно D174.

Старый раздел `#### Prelude opt-out` + `#### Partial prelude opt-in` + `#### Allow prelude shadow`
заменить на единый раздел `#### Prelude control attributes (D174)` — см. текст в конце плана.

**2. Добавить новую секцию `## D174`** между D140 и D134 (порядок не
хронологический, логический) — см. текст в конце плана.

**3. D100 `_module.nv`** — добавить пункт 7 в список Rules:
> 7. `#no_prelude` и `#prelude(...)` в `_module.nv` — наследуются всеми
>    peers (см. D174). Resolver pre-scan'ит `_module.nv` до принятия решения
>    об auto-import prelude.

#### README.md

Добавить строку в «Свежие D-решения»:

```
| D174 | 07-modules.md | Prelude attribute syntax (Plan 107) — `#no_prelude` / `#prelude(names)` / `#allow(shadow)` перед `module`, amends D78 Plan 62.F/62.F.bis; extends D100 (`_module.nv` inheritance) |
```

Добавить D174 в строку файла `07-modules.md` в тематической таблице.

---

### Ф.1 — Parser: добавить новые `#no_prelude`, `#prelude(...)`, `#allow(shadow)`

**Файл:** `compiler-codegen/src/parser/mod.rs`

**Функция:** `parse_module_attrs()` (строки ~585-732)

Добавить распознавание трёх новых ident-паттернов в блок `if !is_forbid &&
!is_cfg && ...` — либо расширить guard, либо сделать ранний `break` для
неизвестных (после добавления новых).

**Изменение в guard (строка ~599):**

```rust
// Было:
let is_no_prelude_attr    = matches!(&next_kind, Some(TokenKind::Ident(n)) if n == "no_prelude");
let is_prelude_attr       = matches!(&next_kind, Some(TokenKind::Ident(n)) if n == "prelude");
let is_allow_attr         = matches!(&next_kind, Some(TokenKind::Ident(n)) if n == "allow");

if !is_forbid && !is_cfg && !is_doc && !is_must_verify_module && !is_proof_budget
    && !is_no_prelude_attr && !is_prelude_attr && !is_allow_attr {
    break;
}
```

**Новые ветки после существующих (после is_proof_budget block):**

```rust
// D174: #no_prelude — полный opt-out из prelude
if is_no_prelude_attr {
    self.bump(); // no_prelude (ident)
    self.expect_newline_or_eof()?;
    let attr_end = self.tokens[self.pos.saturating_sub(1)].span;
    module_attrs.push(ModuleAttr {
        kind: ModuleAttrKind::NoPrelude,
        effects: Vec::new(),
        span: attr_start.merge(attr_end),
    });
    continue;
}

// D174: #prelude(names…) — selective opt-in, ≥1 name
if is_prelude_attr {
    self.bump(); // prelude (ident)
    if !matches!(self.peek().kind, TokenKind::LParen) {
        return Err(Diagnostic::new(
            "expected `(` after `#prelude` \
             (e.g. `#prelude(core, runtime)` or use `#no_prelude` for empty)",
            self.peek().span));
    }
    self.bump(); // (
    // Empty `#prelude()` → explicit compile error (D174: use #no_prelude instead)
    if matches!(self.peek().kind, TokenKind::RParen) {
        return Err(Diagnostic::new(
            "`#prelude()` with empty list is not allowed; \
             use `#no_prelude` to disable all prelude auto-imports (D174)",
            self.peek().span));
    }
    let mut names: Vec<String> = Vec::new();
    loop {
        if matches!(self.peek().kind, TokenKind::RParen) { break; }
        let (n, _) = self.parse_ident()?;
        names.push(n);
        if matches!(self.peek().kind, TokenKind::Comma) {
            self.bump(); // ,
        } else {
            break;
        }
    }
    if !matches!(self.peek().kind, TokenKind::RParen) {
        return Err(Diagnostic::new(
            "expected `)` closing `#prelude(...)` name list",
            self.peek().span));
    }
    self.bump(); // )
    self.expect_newline_or_eof()?;
    let attr_end = self.tokens[self.pos.saturating_sub(1)].span;
    module_attrs.push(ModuleAttr {
        kind: ModuleAttrKind::PartialPrelude(names),  // reuse existing variant
        effects: Vec::new(),
        span: attr_start.merge(attr_end),
    });
    continue;
}

// D174: #allow(shadow) — suppress W_PRELUDE_SHADOW
if is_allow_attr {
    self.bump(); // allow (ident)
    if !matches!(self.peek().kind, TokenKind::LParen) {
        return Err(Diagnostic::new(
            "expected `(` after `#allow` (e.g. `#allow(shadow)`)",
            self.peek().span));
    }
    self.bump(); // (
    let (allow_name, allow_span) = self.parse_ident()?;
    match allow_name.as_str() {
        "shadow" => {}
        _ => return Err(Diagnostic::new(
            format!("`#allow({})` is not a recognized suppressor; \
                     valid value: `shadow` (suppresses W_PRELUDE_SHADOW, D174)",
                     allow_name),
            allow_span)),
    }
    if !matches!(self.peek().kind, TokenKind::RParen) {
        return Err(Diagnostic::new(
            "expected `)` closing `#allow(...)`",
            self.peek().span));
    }
    self.bump(); // )
    self.expect_newline_or_eof()?;
    let attr_end = self.tokens[self.pos.saturating_sub(1)].span;
    module_attrs.push(ModuleAttr {
        kind: ModuleAttrKind::AllowPreludeShadow,  // reuse existing variant
        effects: Vec::new(),
        span: attr_start.merge(attr_end),
    });
    continue;
}
```

**Note:** `ModuleAttrKind` AST variants **не меняются** — `NoPrelude`,
`PartialPrelude`, `AllowPreludeShadow` семантически идентичны; меняется только
точка входа в parser. Imports resolver, lints, type-checker — без изменений.

---

### Ф.2 — Parser: удалить старые inline-клаузы (hard error + migration hint)

**Файл:** `compiler-codegen/src/parser/mod.rs`, строки ~163-218 (clause_attrs loop)

Три match-ветки (`"no_prelude"`, `"partial_prelude"`, `"allow_prelude_shadow"`)
**заменить** на hard compile error с actionable hint:

```rust
"no_prelude" => {
    return Err(Diagnostic::new(
        "inline `no_prelude` clause removed (D174, Plan 107): \
         move to `#no_prelude` before `module` declaration\n  \
         change:  module <path> no_prelude\n  \
         to:      #no_prelude\n  \
         ·        module <path>",
        clause_start));
}
"partial_prelude" => {
    // consume `partial_prelude` ident so span is accurate
    self.bump();
    // skip optional (...) to avoid cascade errors
    if matches!(self.peek().kind, TokenKind::LParen) {
        self.bump(); // (
        let mut depth = 1usize;
        while depth > 0 {
            match self.peek().kind {
                TokenKind::LParen  => { depth += 1; self.bump(); }
                TokenKind::RParen  => { depth -= 1; self.bump(); }
                TokenKind::Newline |
                TokenKind::Eof     => break,
                _                  => { self.bump(); }
            }
        }
    }
    return Err(Diagnostic::new(
        "inline `partial_prelude(...)` clause removed (D174, Plan 107): \
         move to `#prelude(...)` before `module` declaration\n  \
         change:  module <path> partial_prelude(core, runtime)\n  \
         to:      #prelude(core, runtime)\n  \
         ·        module <path>",
        clause_start));
}
"allow_prelude_shadow" => {
    return Err(Diagnostic::new(
        "inline `allow_prelude_shadow` clause removed (D174, Plan 107): \
         move to `#allow(shadow)` before `module` declaration\n  \
         change:  module <path> allow_prelude_shadow\n  \
         to:      #allow(shadow)\n  \
         ·        module <path>",
        clause_start));
}
```

**Важно:** error span должен указывать точно на `partial_prelude`/`no_prelude`/
`allow_prelude_shadow` ident, не на весь module-path. Сообщение содержит
конкретные before/after строки — AI-friendly (actionable hint).

---

### Ф.3 — imports.rs: fix `_module.nv` prelude inheritance

**Файл:** `compiler-codegen/src/imports.rs`

**Проблема (pre-existing bug):** prelude auto-import решение принимается на
строках ~121-127 из `module.attrs`, но inherited attrs из `_module.nv` merging
происходит позже (строки ~463-470). Следовательно `#no_prelude` в `_module.nv`
сейчас **не работает** для peers.

**Решение:** pre-scan `_module.nv` в директории entry-файла до принятия решения
об auto-import. Добавить helper-функцию `preload_module_nv_attrs()`.

**Сигнатура:**

```rust
/// Pre-scan `_module.nv` в директории `entry_path` и вернуть его attrs
/// до полного parse. Используется для early prelude opt-out decision.
///
/// Возвращает `Vec<ModuleAttr>` (может быть пустым если _module.nv отсутствует
/// или не содержит prelude-управляющих attrs). Не вызывает полный resolve —
/// только lexer+parser на `_module.nv`.
fn preload_module_nv_prelude_attrs(
    entry_path: &Path,
    stdlib_dir: &Path,
) -> Vec<crate::ast::ModuleAttr>
```

**Реализация:**

> **API note (важно):** `parse_module_attrs()` — приватный метод `Parser`.
> Из `imports.rs` недоступен. Используем публичный `crate::parser::parse(&src)`,
> который парсит весь файл и возвращает `Module` с заполненными `module.attrs`.
> `_module.nv` маленький (5–20 строк) — overhead полного parse незначителен.
> `imports.rs` уже импортирует `use crate::parser;` (line 13).

```rust
/// D174 / Plan 107 Ф.3: pre-scan `_module.nv` рядом с entry-файлом
/// для early prelude opt-out decision до полного resolve.
///
/// Использует `crate::parser::parse` (публичный API). `parse_module_attrs`
/// приватен для parser-модуля и недоступен снаружи.
///
/// Soft-fail: любая ошибка (файл не найден, parse error) → пустой вектор.
/// Быстрый путь: raw-text check перед полным parse.
fn preload_module_nv_prelude_attrs(entry_path: &Path) -> Vec<crate::ast::ModuleAttr> {
    let dir = match entry_path.parent() { Some(d) => d, None => return vec![] };
    let module_nv = dir.join("_module.nv");
    if !module_nv.exists() { return vec![]; }
    let src = match std::fs::read_to_string(&module_nv) { Ok(s) => s, Err(_) => return vec![] };
    // Fast path: skip full parse если нет prelude-управляющих атрибутов в тексте.
    if !src.contains("#no_prelude") && !src.contains("#prelude") { return vec![]; }
    // Full parse через публичный API.
    match crate::parser::parse(&src) {
        Ok(module) => module.attrs.into_iter()
            .filter(|a| matches!(a.kind,
                crate::ast::ModuleAttrKind::NoPrelude |
                crate::ast::ModuleAttrKind::PartialPrelude(_)))
            .collect(),
        Err(_) => vec![],
    }
}
```

**Где добавить функцию:** вставить сразу ПЕРЕД строкой `pub fn resolve_imports_inline(` (строка ~37 в `imports.rs`). Это private helper в том же файле.

**Edit anchor для вставки функции:**
```rust
// OLD (anchor — не менять, вставить ПЕРЕД этим):
pub fn resolve_imports_inline(
    entry_path: &Path,
    module: &mut Module,
    repo: &Path,
```

**Использование:** в теле `resolve_imports_inline`, ПЕРЕД строкой:

**Edit anchor для вставки вызова** (точная строка в `imports.rs` — это начало
prelude auto-import блока; вставляем ПЕРЕД ним):

```rust
// EXACT ANCHOR (найти эту строку, вставить код ПЕРЕД ней):
    // Plan 35 sub-plan 35.A R27: auto-import `std.prelude` if exists.
```

Вставить:

```rust
    // D174 / Plan 107 Ф.3: pre-scan _module.nv для prelude inheritance.
    // inherited_attrs merge происходит ПОСЛЕ prelude decision (end of fn),
    // поэтому early pre-scan нужен специально для NoPrelude / PartialPrelude.
    // Soft-fail: любые ошибки fs/parse → vec![] (не прерывают compile).
    let module_nv_prelude_attrs = preload_module_nv_prelude_attrs(entry_path);
    // entry-file wins: добавляем только те attrs из _module.nv, чей
    // discriminant отсутствует в уже объявленных attrs entry-файла.
    for attr in module_nv_prelude_attrs {
        if !module.attrs.iter().any(|a| {
            std::mem::discriminant(&a.kind) == std::mem::discriminant(&attr.kind)
        }) {
            module.attrs.push(attr);
        }
    }

    // Plan 35 sub-plan 35.A R27: auto-import `std.prelude` if exists.
```

**Conflict resolution:** если entry-file задаёт `#prelude(core)`, а `_module.nv`
задаёт `#no_prelude` — **entry-file wins** (discriminant-check выше не
добавляет дубликат). Это правильно: per-file override родительского.

---

### Ф.4 — imports.rs: validate `#prelude()` empty list

В `resolve_imports_inline_ex` строки ~132-165, в ветке `Some(names) if names.is_empty()`:

```rust
// D174: пустой список — compile error (parser уже отклоняет #prelude(),
// но inline partial_prelude() могло попасть сюда через старый код).
// После Ф.2 этот путь недостижим, но defensive check для надёжности.
if names.is_empty() {
    return Err(anyhow!(
        "empty prelude list `#prelude()` is not allowed (D174, Plan 107); \
         use `#no_prelude` to disable prelude auto-import\n  \
         in module `{}`",
        module.name.join(".")
    ));
}
```

---

### Ф.5 — Lint: обновить W_PRELUDE_SHADOW диагностику

**Файл:** `compiler-codegen/src/lints.rs`, строки ~835-851 (message в
`lint_prelude_shadow`)

Найти `"allow_prelude_shadow"` в строке suppress-hint и заменить на новый синтаксис:

```rust
// Было:
"Suppress: add `allow_prelude_shadow` clause to module declaration."

// Стало:
"Suppress: add `#allow(shadow)` before `module` declaration (D174)."
```

Аналогично — update комментарий `AllowPreludeShadow` в `ast/mod.rs`:

```rust
// Было в doc-comment:
// Plan 62.F.bis Ф.2: `module X allow_prelude_shadow` (clause syntax, ...)

// Стало:
// Plan 107 D174: `#allow(shadow)` перед `module` declaration (D174).
// Прежняя inline-форма `module X allow_prelude_shadow` удалена.
```

---

### Ф.6 — Migrate: существующие fixtures + std/ комментарии

#### 6.1 — `nova_tests/plan62/*.nv` → обновить syntax

Файлы с inline-клаузами (6 файлов):

| Файл | Что меняется |
|---|---|
| `plan62/no_prelude_no_auto_import.nv` | `module … no_prelude` → `#no_prelude\nmodule …` |
| `plan62/no_prelude_explicit_import.nv` | то же |
| `plan62/partial_prelude_core_only.nv` | `module … partial_prelude(core, runtime)` → `#prelude(core, runtime)\nmodule …` |
| `plan62/partial_prelude_bad_subname.nv` | `module … partial_prelude(c0re)` → `#prelude(c0re)\nmodule …` |
| `plan62/partial_prelude_no_panic_in_core.nv` | `module … partial_prelude(core)` → `#prelude(core)\nmodule …` |
| `plan62/prelude_shadow_suppress.nv` | `module … allow_prelude_shadow` → `#allow(shadow)\nmodule …` |

Также: обновить file-level comments в каждом файле (`module X no_prelude` →
`#no_prelude` + `module X`).

#### 6.2 — `std/prelude/*.nv` — обновить comments

В `std/prelude/core.nv`, `runtime.nv`, `errors.nv`, `collections.nv`,
`protocols.nv`, `effects.nv` — заменить в комментариях:
- `partial_prelude(X)` → `#prelude(X)` (только в комментариях, это doc-strings)

Конкретная строка в каждом файле:
```
// 3) splittable prelude работал (`partial_prelude(X)` opt-in
```
→
```
// 3) splittable prelude работал (`#prelude(X)` opt-in
```

---

### Ф.7 — Tests: `nova_tests/plan107/`

Создать директорию `nova_tests/plan107/` и заполнить:

```
nova_tests/plan107/
├── no_prelude_attr.nv                    # #no_prelude works
├── no_prelude_explicit_import_attr.nv    # #no_prelude + manual import Option
├── prelude_core_attr.nv                  # #prelude(core) → Option/Result visible
├── prelude_multi_attr.nv                 # #prelude(core, runtime, errors)
├── prelude_bad_subname_attr.nv           # #prelude(c0re) → EXPECT_ERROR
├── prelude_empty_error.nv                # #prelude() → EXPECT_ERROR "use #no_prelude"
├── allow_shadow_attr.nv                  # #allow(shadow) suppresses W_PRELUDE_SHADOW
├── old_no_prelude_inline_error.nv        # inline no_prelude → EXPECT_ERROR + hint
├── old_partial_prelude_inline_error.nv   # inline partial_prelude → EXPECT_ERROR + hint
├── old_allow_shadow_inline_error.nv      # inline allow_prelude_shadow → EXPECT_ERROR + hint
└── folder_no_prelude_inherited/          # _module.nv inheritance test
    ├── _module.nv                        # #no_prelude / module folder_no_prelude_inherited.lib
    └── peer.nv                           # module folder_no_prelude_inherited.lib + use Option
                                          # Option invisible → EXPECT_ERROR
```

#### 7.1 — `no_prelude_attr.nv`

```nova
// Plan 107 Ф.7 — `#no_prelude` attribute suppresses auto-import.
// Positive: module compiles, no prelude symbols visible.
// Negative: EXPECT_ERROR_CONTAINS "Option" если обратиться к Option.

#no_prelude
module plan107.no_prelude_attr

fn entry() -> int {
    42
}

test "no_prelude — module compiles without prelude" {
    assert(entry() == 42)
}
```

#### 7.2 — `prelude_core_attr.nv`

```nova
// Plan 107 Ф.7 — `#prelude(core)` делает Option/Result видимыми.

#prelude(core, runtime)
module plan107.prelude_core_attr

test "prelude(core, runtime) — Option visible" {
    ro x Option[int] = Some(42)
    assert(x == Some(42))
}

test "prelude(core, runtime) — Result visible" {
    ro r Result[int, str] = Ok(1)
    assert(r == Ok(1))
}

test "prelude(core, runtime) — panic visible (runtime)" {
    // panic exists — не вызываем, просто проверяем что компилируется
    ro _ = fn() { panic("x") }
    assert(true)
}
```

#### 7.3 — `prelude_empty_error.nv` (negative, EXPECT_ERROR)

```nova
// Plan 107 Ф.7 — `#prelude()` empty list → compile error D174.
// EXPECT_ERROR_CONTAINS: use #no_prelude

#prelude()
module plan107.prelude_empty_error
```

#### 7.4 — `old_no_prelude_inline_error.nv` (negative, EXPECT_ERROR)

```nova
// Plan 107 Ф.7 — inline `no_prelude` removed → compile error with hint.
// EXPECT_ERROR_CONTAINS: inline `no_prelude` clause removed

module plan107.old_no_prelude_inline_error no_prelude
```

#### 7.5 — `old_partial_prelude_inline_error.nv` (negative, EXPECT_ERROR)

```nova
// Plan 107 Ф.7 — inline `partial_prelude(...)` removed → compile error with hint.
// EXPECT_ERROR_CONTAINS: inline `partial_prelude` clause removed

module plan107.old_partial_prelude_inline_error partial_prelude(core)
```

#### 7.6 — `old_allow_shadow_inline_error.nv` (negative, EXPECT_ERROR)

```nova
// Plan 107 Ф.7 — inline `allow_prelude_shadow` removed → compile error with hint.
// EXPECT_ERROR_CONTAINS: inline `allow_prelude_shadow` clause removed

module plan107.old_allow_shadow_inline_error allow_prelude_shadow
```

#### 7.7 — `folder_no_prelude_inherited/_module.nv`

```nova
// Plan 107 Ф.7 — _module.nv #no_prelude propagation test.
// Все peers в этом folder-module наследуют #no_prelude.

#no_prelude
module plan107.folder_no_prelude_inherited
```

#### 7.8 — `folder_no_prelude_inherited/peer.nv` (negative, EXPECT_ERROR)

```nova
// Plan 107 Ф.7 — peer в folder где _module.nv даёт #no_prelude.
// Option должна быть не видна — наследованный #no_prelude.
// EXPECT_ERROR_CONTAINS: Option

module plan107.folder_no_prelude_inherited

fn test_option() -> Option[int] {    // EXPECT_ERROR: Option not in scope
    Some(1)
}
```

---

### Ф.8 — Close

1. `nova test` финальный прогон — ожидаемый результат: все plan62 тесты PASS
   (обновлённый синтаксис), все plan107 тесты PASS/FAIL согласно ожиданиям.
2. Commit: `feat(107): prelude attribute syntax — #no_prelude/#prelude(...)/#allow(shadow)`
3. Обновить `project-creation.txt` + `simplifications.md`.
4. Закрыть memory `project-plan107-status.md`.

---

## Test matrix (итого)

| Test | Тип | Ожидание |
|---|---|---|
| `plan107/no_prelude_attr` | positive | PASS — модуль компилируется |
| `plan107/no_prelude_explicit_import_attr` | positive | PASS — Option через manual import |
| `plan107/prelude_core_attr` | positive | PASS — Option/Result/panic видны |
| `plan107/prelude_multi_attr` | positive | PASS — core+runtime+errors visible |
| `plan107/prelude_bad_subname_attr` | negative | FAIL с "unknown prelude sub-module" |
| `plan107/prelude_empty_error` | negative | FAIL с "use #no_prelude" |
| `plan107/allow_shadow_attr` | positive | PASS — no W_PRELUDE_SHADOW warning |
| `plan107/old_no_prelude_inline_error` | negative | FAIL с "inline `no_prelude` clause removed" |
| `plan107/old_partial_prelude_inline_error` | negative | FAIL с "inline `partial_prelude` clause removed" |
| `plan107/old_allow_shadow_inline_error` | negative | FAIL с "inline `allow_prelude_shadow` clause removed" |
| `plan107/folder_no_prelude_inherited/peer` | negative | FAIL — Option not in scope (inherited #no_prelude) |
| `plan62/*` (6 файлов, обновлены) | positive | PASS — прежняя семантика, новый синтаксис |

Итого: **≥11 новых тестов** + 6 обновлённых. PASS-счёт до/после: ±0 (semantics не меняется).

---

## Edit anchors — точные строки для каждой фазы

Каждый агент должен сначала прочитать полные нужные файлы, затем применять правки.

### Ф.1: `parse_module_attrs()` — добавить три новых атрибута

**Шаг 1.** Заменить guard (добавить три новых `is_X` переменные):

```
FIND (точная строка):
            let is_proof_budget = matches!(&next_kind, Some(TokenKind::Ident(name)) if name == "proof_budget");
            if !is_forbid && !is_cfg && !is_doc && !is_must_verify_module && !is_proof_budget {
                break; // not a module-level attribute
            }

REPLACE WITH:
            let is_proof_budget = matches!(&next_kind, Some(TokenKind::Ident(name)) if name == "proof_budget");
            let is_no_prelude_attr = matches!(&next_kind, Some(TokenKind::Ident(name)) if name == "no_prelude");
            let is_prelude_attr    = matches!(&next_kind, Some(TokenKind::Ident(name)) if name == "prelude");
            let is_allow_attr      = matches!(&next_kind, Some(TokenKind::Ident(name)) if name == "allow");
            if !is_forbid && !is_cfg && !is_doc && !is_must_verify_module && !is_proof_budget
                && !is_no_prelude_attr && !is_prelude_attr && !is_allow_attr {
                break; // not a module-level attribute
            }
```

**Шаг 2.** Добавить три новых блока обработки сразу после блока `is_proof_budget`:

```
FIND (точная строка — конец is_proof_budget блока):
                continue;
            }

            if is_doc {
                // Plan 42.11: `#doc "..."` — module-level documentation line.

INSERT BEFORE (добавить три новых блока ПЕРЕД этой строкой):
            if is_no_prelude_attr { ... }
            if is_prelude_attr { ... }
            if is_allow_attr { ... }
```

Тела блоков — полный код в Ф.1 выше.

---

### Ф.2: inline-клаузы — заменить на hard error

В `resolve_imports_inline` функции (около строки 147-219) найти clause loop.
Точный anchor для каждой замены:

```
FIND:
                    "no_prelude" => {
                        self.bump(); // no_prelude ident
                        let clause_end = ...
                        clause_attrs.push(ModuleAttr {
                            kind: ModuleAttrKind::NoPrelude,
                            ...
                        });
                    }

REPLACE WITH: весь arm → return Err(...) как в Ф.2 выше.
```

Аналогично для `"partial_prelude"` и `"allow_prelude_shadow"` arms.

---

### Ф.3: `imports.rs` — добавить helper + вызов

**Шаг 1.** Добавить `preload_module_nv_prelude_attrs` функцию:

```
FIND (exact — первая строка pub fn):
pub fn resolve_imports_inline(

INSERT BEFORE: всё тело функции из Ф.3 выше (включая doc-comment).
```

**Шаг 2.** Вставить вызов:

```
FIND (exact):
    // Plan 35 sub-plan 35.A R27: auto-import `std.prelude` if exists.

INSERT BEFORE: блок вызова из Ф.3 выше (preload + merge loop + blank line).
```

---

### Ф.4: `imports.rs` — validate empty list

```
FIND (exact):
        } else {
            // Default: full prelude facade.

SEARCH ABOVE for (inside if let Some(names) = partial_prelude_names { branch):
            for name in &names {

INSERT BEFORE loop: defensive empty-list check из Ф.4 выше.
```

Точнее: найти `for name in &names {` внутри ветки `partial_prelude` — вставить
`if names.is_empty() { return Err(...) }` ПЕРЕД этим `for`.

---

### Ф.5: `lints.rs` — replace W_PRELUDE_SHADOW message fragment

```
FIND (exact, многострочная):
                    "[W_PRELUDE_SHADOW] top-level name `{}` shadows a \
                     declaration auto-imported from std.prelude (D29). \
                     User declaration wins — qualify as \
                     `std.prelude.<sub>.{}` to reach the prelude version. \
                     Suppress: add `allow_prelude_shadow` clause to module \
                     declaration, or switch to `no_prelude` / \
                     `partial_prelude(...)` (Plan 62.F).",

REPLACE WITH:
                    "[W_PRELUDE_SHADOW] top-level name `{}` shadows a \
                     declaration auto-imported from std.prelude (D29). \
                     User declaration wins — qualify as \
                     `std.prelude.<sub>.{}` to reach the prelude version. \
                     Suppress: add `#allow(shadow)` before `module` declaration \
                     (D174), or switch to `#no_prelude` / `#prelude(...)` (Plan 107).",
```

`ast/mod.rs` — заменить doc-comment вариантов `NoPrelude`, `PartialPrelude`,
`AllowPreludeShadow`: убрать упоминания `partial_prelude` / `allow_prelude_shadow`
inline-syntax, добавить ссылку на D174.

---

### Ф.6: миграция .nv файлов

Каждый из 6 test fixtures: прочитать, заменить module-line + обновить комментарии.

Каждый из 6 std/prelude/*.nv: заменить только в комментариях
`` `partial_prelude(X)` `` → `` `#prelude(X)` ``.

---

## Appendix: ✅ Spec уже написана

D174 полностью задокументирован в `spec/decisions/07-modules.md#d174`.
D78 §«Prelude opt-out», D100 rule 7, README — обновлены.

<!-- old appendix text removed — spec is live in 07-modules.md -->

## D174. Prelude control attributes — `#no_prelude`, `#prelude(...)`, `#allow(shadow)`

### Что

Три **module-level атрибута** для управления auto-import prelude (D26).
Идут **ПЕРЕД** `module` declaration — консистентно с `#cfg` / `#doc` / `#forbid`
(D99 §«Позиция»):

```nova
#no_prelude
module my.realtime

#prelude(core, runtime)
module my.dsl

#prelude(core, runtime, errors)
#allow(shadow)
module collections.range
```

Amends D78 §«Prelude opt-out» + Plan 62.F + Plan 62.F.bis: удалены
inline-клаузы `no_prelude` / `partial_prelude(...)` / `allow_prelude_shadow`
на `module`-line (Plan 107).

### Атрибуты

#### `#no_prelude` — полный opt-out

```nova
#no_prelude
module my.realtime

// никаких авто-импортов; даже Option/Result нужно явно:
import std.prelude.core.{Option, Result}
```

Применение: real-time/embedded (prelude содержит GC-код), bootstrap уровни,
обучающие примеры с explicit visibility.

Без `#no_prelude` — стандартный prelude в скоупе (D26).

#### `#prelude(names…)` — selective opt-in

```nova
#prelude(core, runtime)
module my.dsl

// Видимо: Option/Result/Some/None/Ok/Err/Error/Ordering (core)
// + panic/exit/assert/debug_assert (runtime).
// НЕ видимо: RuntimeError (errors), Iter (collections),
//            From/Hashable/… (protocols), Fail[E]/Time/Mem (effects).
```

Валидные имена (`names`):

| Имя | Содержимое |
|---|---|
| `core` | `Option`/`Result`/`Some`/`None`/`Ok`/`Err`/`Error`/`Ordering` |
| `runtime` | `panic`/`exit`/`assert`/`debug_assert`/`print`/`println` |
| `errors` | `RuntimeError` (6 variants) + `ReadBufferError` |
| `collections` | `Iter[T]` protocol |
| `protocols` | `From`/`Into`/`Hashable`/`Equatable`/`Comparable`/`Display` |
| `effects` | `Fail[E]` + `Time` + `Mem` |

Имена валидируются resolver'ом — `#prelude(badname)` → compile error со
списком валидных имён. Пустой список `#prelude()` → **compile error**:
`"use #no_prelude for empty prelude"`. Список должен содержать ≥1 имя.

Применение: bootstrap уровни (selective opt-in без ручного `import` каждого
`Option`/`Result`), DSL слой (нужны Option/Result, но не protocols).

#### `#allow(shadow)` — suppress W_PRELUDE_SHADOW

```nova
#allow(shadow)
module my.dsl

type Option { foo int }         // user-declaration — warning silenced
const PRELUDE_VERSION int = 99  // shadowing prelude const — warning silenced
```

Подавляет structured `W_PRELUDE_SHADOW` lint (D125) на уровне модуля.
Shadowing по-прежнему допустим (user-decl wins); `#allow(shadow)` убирает
предупреждение.

Применение: embedded DSL с переопределением `Option`/`Result` с осознанным
intent'ом, test fixtures для explicit shadowing behavior, bootstrap слои.

Item-level suppress (`#[allow(shadow)]` перед конкретным объявлением) —
deferred (требует generic attribute parser).

### Комбинирование

Атрибуты можно комбинировать — каждый на отдельной строке перед `module`:

```nova
#prelude(core, runtime)
#allow(shadow)
module my.dsl
```

Нельзя комбинировать `#no_prelude` + `#prelude(...)` на одном файле —
compile error: `"conflicting prelude attributes: #no_prelude and #prelude(...)
cannot both be present"`.

### `_module.nv` inheritance (D174 + D100)

`#no_prelude` и `#prelude(...)` в `_module.nv` наследуются всеми peers
folder-module — resolver pre-scan'ит `_module.nv` до принятия решения об
auto-import prelude (Ф.3 Plan 107):

```
realtime/
├── _module.nv   #no_prelude + module realtime.lib
├── sched.nv     module realtime.lib → inherits #no_prelude
└── timer.nv     module realtime.lib → inherits #no_prelude
```

Если peer сам декларирует `#prelude(core)`, а `_module.nv` даёт `#no_prelude`
— **peer-file wins** (per-file override folder-level).

### Грамматика

```
file-level-attr := prelude-attr | allow-attr | cfg-attr | doc-attr | …
prelude-attr    := "#no_prelude"
                 | "#prelude" "(" prelude-names ")"
prelude-names   := prelude-name ("," prelude-name)*    // ≥1
prelude-name    := "core" | "runtime" | "errors"
                 | "collections" | "protocols" | "effects"
allow-attr      := "#allow" "(" allow-target ")"
allow-target    := "shadow"                            // extensible
```

### Почему

1. **Консистентность с D99.** Все module-level attrs идут ПЕРЕД `module` —
   `#cfg`, `#doc`, `#forbid`. Inline-клаузы были единственным исключением.
2. **`_module.nv` inheritance.** Pre-module attrs автоматически попадают в
   inherited_attrs propagation (D100). Inline-клаузы не могли.
3. **Короче.** `#prelude(core)` vs `partial_prelude(core)` — `partial_` лишнее.
   `#allow(shadow)` vs `allow_prelude_shadow` — с 19 до 13 символов.
4. **Нет footgun.** `partial_prelude()` молча трактовалось как `no_prelude` —
   теперь compile error с actionable hint.
5. **Расширяемость.** `#allow(X)` — extensible до других suppressors
   (item-level, future lints). `#prelude` — extensible до `+name`/`-name`
   additive form если появится потребность.

### Что отвергнуто

- **Inline-клаузы (Plan 62.F / 62.F.bis оригинальный)** — удалены (D174).
  Hard error с migration hint при встрече старого синтаксиса.
- **`prelude { ... }` блок** — избыточен; список имён достаточен.
- **`+name`/`-name` аддитивный синтаксис** (`#prelude(-effects)` чтобы
  убрать из full prelude) — deferred. Текущий opt-in достаточен; аддитивный
  usеful если prelude будет очень большим. Q-prelude-additive-syntax.
- **`#prelude(*)` для explicit full prelude** — не нужен, default уже full.
  Deferred для случая «документирую явно».

### Связь

- D26 (08-runtime.md) — prelude items.
- D78 (07-modules.md) §«Prelude opt-out» — amended.
- D99 (09-tooling.md) — `#cfg` position rule; D174 следует тому же.
- D100 (07-modules.md) — `_module.nv` inheritance.
- D125 (08-runtime.md) — `W_PRELUDE_SHADOW` lint.
- Plan 107 — реализация.

### Цена

1. **Breaking change.** Все файлы с inline-клаузами (15 на 2026-05-27) —
   compile error до миграции. Nova pre-production → grace period не нужен.
2. **Pre-scan `_module.nv`.** Дополнительный fs-read + partial parse
   `_module.nv` в hot path imports resolver'а. Cost: один extra `fs::read_to_string`
   + lexer на обычно небольшой файл (~5-20 строк). Mitigated:
   raw-text check на `"#no_prelude"`/`"#prelude"` before full parse.
```
```

---

## Открытые вопросы

- **Q-prelude-additive-syntax.** Нужна ли форма `#prelude(-effects)` для
  «full prelude минус один модуль»? Если prelude вырастет значительно —
  да. Пока отложено.
- **Item-level `#[allow(shadow)]`.** Deferred до generic attribute parser.
  Текущий `#allow(shadow)` module-level. Q-item-level-allow-attr.
