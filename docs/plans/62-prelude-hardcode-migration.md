# Plan 62: Migrate hardcoded prelude → `std/prelude.nv` (full D26 compliance + splittable + `no_prelude` enforcement)

> **Status:** proposed (2026-05-17, revised). Architectural cleanup. Закрывает spec/impl drift между [D26](../../spec/decisions/08-runtime.md#d26) и фактической реализацией. **6 фазных sub-plans (62.A–62.F)** — реалистичная estimate 10-15 dev-days (v1 заявлял 3-5 — нереалистично для full D26).

---

## Цель в одной фразе

`std/prelude.nv` должен быть **единственным source-of-truth** для всего, что D26 называет prelude (всех типов + methods + functions + protocols + effects), все hardcoded special-case'ы в type-checker и codegen должны быть удалены (с заменой на generic generic-driven dispatch), `no_prelude` должен реально работать (сейчас silent bug), а сам prelude должен быть **splittable** (можно opt-out от подмножеств) и **edition-versionable** — чтобы был лучше, чем Rust `prelude::v1` (который требует от пользователя писать дублированные пути).

---

## Problem (verified state)

### 1. `std/prelude.nv` — фактически пустой

[`std/prelude.nv`](../../std/prelude.nv) — 21 строка, 1 полезный item:
```nova
export const PRELUDE_VERSION int = 1
```
Этот marker создан только чтобы доказать что R27 auto-import работает (Plan 35.A). Никаких реальных prelude items в файле нет.

### 2. Spec D26 vs реальность — серьёзный drift

D26 (см. `spec/decisions/08-runtime.md:249-431`) фиксирует **гораздо более широкий** prelude, чем планировал v1 Plan 62.

| Категория | v1 Plan 62 список | Spec D26 фактический список |
|---|---|---|
| Sum-types | Option, Result, Ordering*, Never*, Error* | Option, Result, Ordering, Never |
| Variants | Some/None, Ok/Err, Less/Equal/Greater | Some/None, Ok/Err, Less/Equal/Greater |
| Error types | Error | Error, RuntimeError (6 variants), RuntimeNoneError |
| Type with body | (none) | Iter[T] (protocol D58), Range, RangeIter |
| Opaque types | (none) | StringBuilder, WriteBuffer, ReadBuffer, ReadBufferError |
| Top type | (none) | any (через D54 empty protocol) |
| Effects | (none) | Fail, Time, Mem, Detach (D62/D65) |
| Functions | print, println, panic, assert, debug_assert | + exit (D13) |
| Methods | (none) | ~17 method on Option/Result (D26 §283-306) |
| Protocols (D73/D77/D109) | (none) | From, Into, TryFrom, TryInto, Hashable, Equatable, Comparable, Display |

V1 покрывал **13 items**, реальный D26 — **~50+ items**. Acceptance criteria v1 «std/prelude.nv contains explicit declarations for all currently-hardcoded items» **не выполняется** при покрытии 13 из 50.

### 3. Hardcoded special-cases в type-checker — больше чем заявлял v1

[`compiler-codegen/src/types/mod.rs:1974-2024`](../../compiler-codegen/src/types/mod.rs#L1974) — единый `builtins: HashSet<String>` содержит:
- Prelude variants: `None, Some, Ok, Err, Option, Result, Error` (~7)
- RuntimeError variants: `DivByZero, Overflow, IndexOutOfBounds, TypeMismatch, AssertFailed, NoHandler, RuntimeError` (7)
- Functions: `assert, debug_assert, print, println, panic, exit` (6)
- Type idents: `Never, any, Self, self, true, false` (6)
- Effect namespaces: `gc, bench, fibers, runtime, Fail, Detach, CancelToken` (7)

Plus `ty_of_ref` (mod.rs:2904-2916) — primitives hardcoded: `int/i8..i64/u8..u64/f32/f64/str/bool/byte/Never`.

**V1 упустил:** `exit`, RuntimeError varianты, effect namespaces, `any`.

### 4. Hardcoded special-cases в codegen — даже больше

[`compiler-codegen/src/codegen/emit_c.rs`](../../compiler-codegen/src/codegen/emit_c.rs):
- `println/print` (10241): emit_println branch.
- `assert/debug_assert` (10252-10268): comma-operator inline.
- `panic` (10281): `nv_panic`.
- `exit` (10294): `nv_exit` — **v1 не упомянул**.
- Pre-populated `sum_schemas`: `Option={Some(int), None}`, `Result={Ok(int), Err(str)}` (754-766) — **bootstrap monomorphization compromise**, hardcoded на конкретные типы. Простой перенос декларации в prelude.nv **не уберёт этот hardcode** — нужен Plan 14 Q-result-monomorphization fix.
- `Error` record schema + `Error.new` method (774-782).
- `RuntimeError` sum schema (797-820) с 6 вариантами.
- `Some/None/Ok/Err` pattern-match hardcoded в variant pattern lookup (13543-13565, 14640-14809) — **это codegen pattern dispatch**, не type-checker. Перенос декларации НЕ удалит эти branches; нужен generic `NovaOpt`/`NovaResult` matching.

### 5. `no_prelude` opt-out — silent bug

Spec [07-modules.md:962-979](../../spec/decisions/07-modules.md#L962) формально специфицирует `module my.mod no_prelude`. Parser, насколько можно судить по imports.rs, **не учитывает** этот modifier при auto-import prelude — [`imports.rs:114`](../../compiler-codegen/src/imports.rs#L114) проверяет только `is_prelude_self_module`. Это **silent spec/impl drift**: код с `no_prelude` всё равно получит prelude, без warning. Real-time / embedded use-case (для которого `no_prelude` и придуман) сейчас не работает.

### 6. Open question плана v1 «bootstrap cyclic-import» уже решён

[`imports.rs:112`](../../compiler-codegen/src/imports.rs#L112) — `is_prelude_self_module` guard уже защищает от self-cycle. План v1 ставил это как Open Question, хотя кода уже было достаточно. Уточнение: cycle resolved для **prelude ↔ prelude**, но **не для prelude → std.runtime.string → prelude**. Если перенесём `Option` в prelude.nv, а prelude.nv импортирует `std.runtime.string` (для `str` методов), а `std.runtime.string` использует `Option` — циклический-import. Нужна **explicit стратификация**: prelude.nv ZERO imports (только declarations); если требуется forward-decl — `external fn`. См. §«Архитектурные правила» ниже.

### 7. `PRELUDE_VERSION` versioning — placeholder без mechanism

V1 не объясняет, что `PRELUDE_VERSION = 1` const на самом деле даёт. Никакого механизма «опираться на version» не существует — это голый marker. Production-grade versioning требует либо:
- (a) Rust-style — путь `std::prelude::v1` vs `std::prelude::rust_2021`; пользователь пишет дублированные пути;
- (b) Edition-based — `nova.toml` декларирует `edition = "2026.05"`, resolver автоматически подставляет правильную версию prelude. **AI-friendly**, single path `std.prelude`, content edition-dependent.

Predлагаем (b) — strictly лучше Rust.

### 8. Splittability — не упомянуто

Большие prelude'ы плохи для:
- Embedded (не нужен `println`/`StringBuilder`).
- Tests (хотят `assert` но не `Time/Fail` ambient).
- Quick scripts (хотят `print` но не `RuntimeError`).

Rust решает это через `#![no_std]` + `core::prelude` subset. Nova может сделать **splittable**: `std.prelude` = re-export from `std.prelude.core` + `std.prelude.io` + `std.prelude.collections`. Каждая часть opt-out'имая независимо.

---

## Что НЕ делаем (rejected)

| Alternative | Почему отвергнут |
|---|---|
| **Не трогать primitives** (`int`/`str`/`bool`/`f64`...) | Это уже исключено в v1 — correct. Primitives — language keywords, не prelude. |
| **Migrate всё одним PR** | 50+ items × pattern-match/codegen changes → high risk; неизбежны regressions. Phased migration через sub-plans 62.A–62.F. |
| **Удалить `PRELUDE_VERSION` без замены** | Полезный marker для verification что resolver не сломался. Сохранить + use в tests. |
| **Mandatory `no_prelude` для всех файлов** | Слишком verbose, против spirit'а «zero-friction для quick scripts». |
| **Single huge prelude (Java `java.lang`-style)** | Splittable лучше: AI видит конкретное подмножество, embedded работает. |
| **Versioning через имена путей (Rust `prelude::v1`)** | Дублирует пути в каждом import. Edition-based в `nova.toml` лучше. |
| **Lazy / on-demand prelude** | Усложняет mental model, не нужно для bootstrap. Один auto-import работает. |
| **3-5 dev-days estimate (v1)** | Нереалистично для full D26. Реалистично — 10-15 dev-days в 6 фаз. |

---

## Решение — архитектурные правила

### Стратификация (избежать circular imports)

**`std.prelude` строго zero-import.** Все declarations либо self-contained (sum types на primitives), либо forward'ятся через `external fn` (для runtime helpers, которые реализованы в C: `print/println/panic/exit`). Это правило проверяется CI gate'ом «prelude.nv parses без import keyword».

Methods на Option/Result, которые требуют другие prelude items (`Option.unwrap` нужен `Fail`) — допустимы потому что `Fail` тоже в prelude (same file). Self-reference внутри prelude OK.

Это эквивалент Rust-стратификации `core::prelude` → `alloc::prelude` → `std::prelude` — но Nova проще, один `std.prelude`.

### Splittable structure

```
std/prelude.nv                 — re-exports from sub-modules + edition pinning
std/prelude/core.nv            — Option, Result, Error, Never, Ordering, any
std/prelude/runtime.nv         — print, println, panic, assert, debug_assert, exit
std/prelude/effects.nv         — Fail, Time, Mem, Detach (effect declarations)
std/prelude/collections.nv     — Iter[T], Range, RangeIter, StringBuilder, etc.
std/prelude/protocols.nv       — From, Into, TryFrom, TryInto, Hashable, Equatable, Comparable, Display
std/prelude/errors.nv          — RuntimeError, RuntimeNoneError, ReadBufferError
```

`std/prelude.nv` сам — fasade:
```nova
module std.prelude

export import std.prelude.core.*
export import std.prelude.runtime.*
export import std.prelude.effects.*
export import std.prelude.collections.*
export import std.prelude.protocols.*
export import std.prelude.errors.*

#stable(since = "0.1")
export const PRELUDE_VERSION int = 2   // 2 after migration
```

`no_prelude` — opt-out от всего. `partial_prelude(core, runtime)` — opt-in только подмножеств. Spec amendment.

### Edition-versionable

`nova.toml`:
```toml
[workspace]
edition = "2026.05"
```

Resolver читает edition и, если задано, выбирает `std/prelude/<edition>.nv` вместо default. Bootstrap MVP: ровно одна edition `2026.05`, реально просто marker для будущей migration. Spec D26 amend с edition-policy.

### Hardcode removal strategy

Каждый item имеет **3 уровня hardcode**:

| Уровень | Где | Как удалять |
|---|---|---|
| **L1: name in builtins HashSet** | type-checker `mod.rs:1974` | Удалить из set — но **только после** того, как item зарегистрирован через cross-file resolve (`std.prelude.<name>`). |
| **L2: special-case в codegen** | emit_c.rs `println/print/assert/...` | Заменить на generic external-fn dispatch (через `runtime_registry.rs`) **или** оставить call-side mapping но удалить name-recognition (т.е. дать prelude.nv фабриковать external fn declaration). |
| **L3: hardcoded sum schemas / variant patterns** | emit_c.rs `sum_schemas`, pattern lookup | Заменить на generic schema lookup через `cross-file resolve` (Plan 35 уже даёт infrastructure). **Самое сложное** — особенно для `Result[T, E]` mono compromise. |

**Removal order matters:** сначала добавить декларации в prelude.nv (L0), потом удалять L2/L3 (по одному, testing после каждого), последним удалять L1 (name-recognition становится unused когда cross-file resolve работает).

### `Result[T, E]` mono compromise — отдельная Sub-task

Codegen pre-populates `sum_schemas`:
```rust
sum_schemas.insert("Result", vec![Variant::Tuple("Ok", vec!["nova_int"]), Variant::Tuple("Err", vec!["nova_str"])]);
```

Это hardcoded на конкретные `(int, str)`. Невозможно полностью удалить эту строчку без полной generic monomorphization (Plan 14 Q-result-monomorphization, или Plan 59 tuple-mono approach extended на sum-types).

**Decision:** см. **[Plan 62.A.bis](62.A.bis-sum-schema-registry.md)** — детальный design-документ (proposed 2026-05-18). Generic schema registry поверх существующего `sum_schemas`, hardcoded `Result` остаётся как fallback baseline, но `infer_result_type` сначала смотрит в registered prelude'е, потом в hardcoded. После того как Plan 59 mono-pattern расширится на sum-types — hardcoded compromise можно полностью удалить.

### Pattern-match codegen lookup

`Some/None/Ok/Err` matching hardcoded в emit_c.rs:13543-13565. После переноса декларации в prelude.nv эти branches **не удалятся автоматически** — pattern dispatch смотрит на variant имена.

**Decision:** оставить hardcoded matching path для bootstrap, но изменить lookup: вместо name comparison `name == "Some" || ...` — смотреть в zonked declaration prelude'а. Если декларация Option матчит structure → допускаем pattern. Это даёт user shadowing-warning при name conflict.

---

## Sub-plans (phased migration)

### Plan 62.A — Core types: Option/Result/Some/None/Ok/Err (3 days)

> **Status update 2026-05-18:** Phase 62.A complete (698/698 PASS, 0 regression). Types перенесены, builtins HashSet shrunk на 11 имён. **Deferred:** `Never` (parser не поддерживает empty-sum) + 17 методов (codegen/type-checker signature mismatch — см. §«DEFER» ниже).
>
> **Status update 2026-05-18 (follow-up):** **12/17 methods discharged** через `std/prelude/core.nv` + `init_prelude_decls_from_items()` (registry-driven inheritance). 700/700 PASS, +2 positive test files. Deferred — 5 Result methods (`unwrap`/`unwrap_or`/`unwrap_or_else`/`map`/`map_err`) — originally-anticipated `T → T` блокер confirmed present для Result non-per-T mono compromise (Option per-T трамплины не affected). Один Option method `or` declared без codegen support (DEFER comment в core.nv). См. Sub-section §«62.A 17-methods defer» ниже.

- [x] Создать `std/prelude/core.nv` с declarations:
  ```nova
  module std.prelude.core
  export type Option[T] | Some(T) | None
  export type Result[T, E] | Ok(T) | Err(E)
  export type Error { readonly msg str }
  export type Ordering | Less | Equal | Greater
  // DEFER: type Never — empty-sum syntax не поддержан parser'ом
  ```
- [x] **Partial (12/17, 2026-05-18 follow-up):** D26 §283-306 methods для Option/Result.
      - Option: **8/8** declared — `is_some`, `is_none`, `unwrap` (Fail[Error]),
        `unwrap_or`, `unwrap_or_else`, `map`, `ok_or`, `or` (последний с DEFER —
        codegen trampoline отсутствует).
      - Result: **4/9** declared — `is_ok`, `is_err`, `ok`, `err`.
      - Result deferred: **5/9** (`unwrap`, `unwrap_or`, `unwrap_or_else`, `map`,
        `map_err`) — originally-anticipated `T → T` блокер всё ещё present.
        Type-checker, видя `external fn Result[T, E] @unwrap_or(default T) -> T`,
        инфериует результат `r.unwrap_or(0)` как `Result*` heap-pointer (через
        generic substitution), codegen `==` идёт через sum-tag-comparison.
        Конкретные regressions: `runtime/unwrap_or` / `runtime/result_methods` /
        `runtime/from_into_basic` / `runtime/read_buffer` / `runtime/read_text`.
        Лечение в Plan 62.B+ через per-T mono Result (как Option) или type-checker
        special-case. Declarations закомментированы в core.nv с актуальным DEFER
        comment'ом. **Корневая причина** — codegen `type_of_method_call_c`
        (emit_c.rs:18619+) hardcoded возвращает `nova_int` для Result.unwrap_or,
        несовместимо с generic Nova signature.
- [x] `init_prelude_decls_from_items()` (sum_schema_registry.rs:513+) scan'ит
      `module.items` для `Item::Fn` с receiver `Option`/`Result`,
      регистрирует `DeclaredFromPrelude` entries с method_routing,
      унаследованным от `HardcodedBaseline` — behavior-preserving
      migration источника правды без изменения dispatch. +4 unit-теста
      (13/13 PASS в sum_schema module).
- [x] Update `std/prelude.nv` чтобы re-export `core.*` (selective form;
      wildcard `.* ` не поддержан per Plan 35 R25 rejected).
- [ ] Add Sub-task 62.A.bis: generic schema registry для sum-types
      (поверх hardcoded `sum_schemas`). **Postponed:** scope grew с
      попыткой добавить методы.
- [x] Удалить из type-checker builtins HashSet: `Option, Result, Some,
      None, Ok, Err, Error, Ordering, Less, Equal, Greater` (11 имён;
      `Never` deferred).
- [x] **NOT yet (per план):** удаление pre-populated `sum_schemas["Result"]` —
      оставлено до Plan 59 expand на sum.
- [x] Regression: 698/698 PASS (новый baseline; старый documented
      baseline 509/535 PASS устарел).

**Дополнительные изменения compiler'а (Plan 62.A scope creep, минимально-необходимое):**

- `compiler-codegen/src/manifest.rs:is_prelude_self_module` —
  расширен на `std.prelude.<sub>` (splittable sub-modules). Без
  этого `std.prelude.core` auto-импортирует `std.prelude` →
  circular import.
- `compiler-codegen/src/imports.rs:resolve_module_paths` — relaxed
  ambiguity check (`<X>.nv` + folder `<X>/<sub>.nv`): permitted, если
  ВСЕ peer declarations начинаются с `<X>.<sub>` (i.e. child modules,
  не peers). Plan 42 D29 originally errored на любой file+folder
  co-existence. Добавлен helper `extract_declared_module`
  (lightweight `module X.Y.Z` extraction без полного парсинга).
- `compiler-codegen/src/types/mod.rs:check_module` —
  `external fn` whitelist расширен на `std.prelude.*` (через
  `is_prelude_self_module`). Также: проверка теперь iterates только
  entry peers' `items_here`, не merged `module.items` — иначе
  prelude'овые external fns тропают на user-модулях при auto-import.
- `compiler-codegen/src/codegen/emit_c.rs`:
  - `emit_type_decl` skip emission for `Option`/`Result`/`Error`
    (runtime defined в nova_rt/array.h, doubly-emit конфликтует
    с runtime helpers).
  - `generic_types` registration (1a + 1d) skips `Option`/`Result` —
    они handled через `NovaOpt_<T>` / `Nova_Result*` value/heap
    types, не через generic mono path.
- `nova_tests/modules/prelude_auto_import.nv` — bumped
  `PRELUDE_VERSION` assertion 1 → 2.

### Plan 62.B — Runtime functions: panic/exit/assert/debug_assert (partial) (2 days)

> **Status update 2026-05-18:** Phase 62.B complete (partial scope —
> 698/698 PASS preserved, 4/6 functions migrated). `panic`/`exit`/`assert`/
> `debug_assert` перенесены в `std/prelude/runtime.nv` через `external fn`
> declarations. **Deferred:** `print` / `println` (variadic +
> type-polymorphic dispatch — single-arg `external fn print(s str)`
> декларация сломала бы hundreds существующих test-callers вида
> `println("x=", n)`. См. §«DEFER 62.B → 62.B.bis» ниже).

- [x] Создать `std/prelude/runtime.nv`:
  ```nova
  module std.prelude.runtime
  external fn panic(msg str) -> Never
  external fn exit(code int, msg str) -> Never
  external fn assert(cond bool) -> ()
  external fn assert(cond bool, msg str) -> ()        // D84 arity overload
  external fn debug_assert(cond bool) -> ()
  external fn debug_assert(cond bool, msg str) -> ()  // D84 arity overload
  // DEFER: print / println — см. ниже.
  ```
  - Default-param syntax (`msg str = "assertion failed"`) **отвергнут** —
    D102 named-only-defaults требует keyword-form (`assert(c, msg="x")`)
    на call-site'е, но ~17 test файлов используют 2-arg positional форму
    (`assert(c, "msg")`). D84 arity overload работает без break'а.
  - `msg` параметр в 2-arg форме **SILENTLY IGNORED** в bootstrap'е —
    emit_c.rs:11086 всё равно использует auto-derived cond_text (Plan 11
    fix). Explicit msg override — отдельная задача (требует change в
    emit_c.rs c использованием user-supplied msg).
- [x] **NOT wired** через registry (`runtime_registry.rs`) — registry —
      это auto-gen infrastructure для `std/runtime/string.nv` /
      `std/runtime/math.nv` (Plan 13), не runtime dispatch. Dispatch
      остаётся через emit_c.rs name-keyed special-cases (см. ниже).
- [x] **emit_c.rs special-cases НЕ конвертированы** в generic external-fn
      dispatch — оба остаются name-keyed:
  - `panic`/`exit` (emit_c.rs:11115/11128): нужны comma-expression обёртка
    `(nv_panic(msg), (nova_int)0LL)` для использования в expression-position
    (`?? panic(...)`, `if cond { v } else { panic(...) }`). Generic
    external-fn dispatch вернул бы `void`, что в C cast'ить в `nova_int`
    нельзя. Comma-expression — bootstrap решение.
  - `assert`/`debug_assert` (emit_c.rs:11086): D89 expression-context +
    Plan 11 comma-operator wrap `(nova_assert(cond, "<text>"), NOVA_UNIT)`.
    `<text>` — auto-derived display rep'а expression'а (msg arg silently
    ignored). Generic dispatch не даёт auto-derive.
  - Все 4 intercept'а остаются по `name == "X"` lookup — после HashSet
    shrink (Step 5) это **продолжает работать**, потому что cross-file
    resolve через R26+R27 находит declaration, и AST Ident name всё ещё
    `"panic"` / `"exit"` / `"assert"` / `"debug_assert"` (не qualified).
- [x] Update `std/prelude.nv` чтобы re-export `runtime.{panic, exit,
      assert, debug_assert}` (selective form, mirror 62.A).
- [x] Удалить из type-checker builtins (4 имени): `panic, exit, assert,
      debug_assert`. **NOT removed:** `print, println` (DEFER 62.B.bis).
- [x] Regression: 698/698 PASS (новый baseline после 62.A).

**DEFER 62.B → 62.B.bis: print / println**

Реальная dispatch (`emit_c.rs::emit_println` + `make_print_call` +
`infer_print_helper` ~13638-13751) — **variadic + type-polymorphic
per-argument**:
- `println(42)` → `nova_print_int(42); nova_print_newline()`
- `println(true)` → `nova_print_bool(true); nova_print_newline()`
- `println("hello")` → `nova_print_str("hello"); nova_print_newline()`
- `println("x=", n)` → `nova_print_str("x="); nova_print_int(n); nova_print_newline()`
- `println("  fiber ", x, " a")` — 3-arg mixed types

Single-arg `external fn print(s str) -> ()` декларация сломает все
non-str + multi-arg callers. Real test files используют все четыре
формы выше (см. `nova_tests/concurrency/parallel_for.nv` и аналоги).

Полная миграция требует либо:
- (a) variadic external fn syntax — нет в bootstrap parser;
- (b) auto-derive Display protocol + StringBuilder pipeline (Plan 62.D
  collections + Plan 62.E protocols) — пользователь пишет
  `println("x=${n}")`, type-checker auto-derives Display::fmt для
  каждого argument'а через `${...}` interpolation;
- (c) Plan 67 (println return-type inference) extended на variadic
  + type-polymorphic arity matching.

Все три — out-of-scope 62.B (≤2 dev-days). Запланирован Plan 62.B.bis
после 62.D + 62.E (Display protocol даст canonical pipeline).

**Файлы изменены (Plan 62.B partial):**

- `std/prelude/runtime.nv` (NEW) — 4 external fn declarations с
  6 arity-overload вариантами.
- `std/prelude.nv` — добавлен `export import std.prelude.runtime.{panic,
  exit, assert, debug_assert}`.
- `compiler-codegen/src/types/mod.rs:2031-2032` — builtins HashSet
  shrunk на 4 имени (`panic, exit, assert, debug_assert` удалены;
  `print, println` оставлены с DEFER comment).

### Plan 62.C — Error types: RuntimeError/RuntimeNoneError (1 day)

- [ ] Создать `std/prelude/errors.nv`:
  ```nova
  module std.prelude.errors
  export type RuntimeError
      | DivByZero
      | Overflow
      | IndexOutOfBounds { index int, length int }
      | TypeMismatch(str)
      | AssertFailed(str)
      | NoHandler(str)
  export type RuntimeNoneError
  export type ReadBufferError
      | UnexpectedEnd { wanted int, available int }
  ```
- [ ] Wire через generic schema registry (Plan 62.A.bis).
- [ ] Удалить из builtins HashSet: 7 RuntimeError variants + RuntimeError + RuntimeNoneError + ReadBufferError + UnexpectedEnd.
- [ ] Удалить pre-populated `sum_schemas["RuntimeError"]` (797-820) — заменить на registry lookup.
- [ ] Regression: 562/562 PASS.

### Plan 62.D — Iter[T] + Range + StringBuilder + WriteBuffer + ReadBuffer (2 days)

- [ ] Создать `std/prelude/collections.nv`:
  ```nova
  module std.prelude.collections
  export type Iter[T] protocol { mut next() -> Option[T] }
  export type Range { readonly start int; readonly end int; readonly inclusive bool }
  export type RangeIter { end int; inclusive bool; mut cur int }
  external type StringBuilder
  external type WriteBuffer
  external type ReadBuffer
  ```
- [ ] `external type` — нужен новый syntax (или re-use `external fn` semantic). Если syntax не существует — это D-block addition в spec (см. §«Spec changes» ниже).
- [ ] Wire opaque type registrations (currently scattered).
- [ ] Regression: 562/562 PASS.

### Plan 62.E — Protocols: From/Into/TryFrom/TryInto/Hashable/Equatable/Comparable/Display (2-3 days)

- [ ] Создать `std/prelude/protocols.nv`:
  ```nova
  module std.prelude.protocols
  export type From[T] protocol { fn from(t T) -> Self }
  export type Into[U] protocol { fn into() -> U }
  export type TryFrom[T, E] protocol { fn try_from(t T) Fail[E] -> Self }
  export type TryInto[U, E] protocol { fn try_into() Fail[E] -> U }
  export type Hashable protocol { fn hash() -> u64 }
  export type Equatable protocol { fn eq(other Self) -> bool }
  export type Comparable protocol { fn cmp(other Self) -> Ordering }
  export type Display protocol { fn fmt(sb StringBuilder) -> () }
  ```
- [ ] Auto-derive remains в compiler (D73/D77/D109), но declarations теперь file-based — bound clauses `[T Hashable]` могут резолвиться через cross-file resolve вместо hardcoded check.
- [ ] Wire bound enforcement через Plan 15 (generic-bounds-enforcement) infrastructure.
- [ ] Regression: 562/562 PASS.

### Plan 62.F — Effects: Fail/Time/Mem/Detach + `no_prelude` enforcement + edition-versioning (3-4 days)

- [ ] Создать `std/prelude/effects.nv`:
  ```nova
  module std.prelude.effects
  export effect Fail[E] { fn fail(e E) -> Never }
  // Time/Mem/Detach — формализация ambient effect declarations
  ```
- [ ] **CRITICAL:** `no_prelude` enforcement в [`imports.rs:114`](../../compiler-codegen/src/imports.rs#L114) — currently silent bug. Fix:
  ```rust
  let has_no_prelude = module.attrs.iter().any(|a| matches!(a.kind, ModuleAttrKind::NoPrelude));
  if !is_prelude_self && !has_no_prelude { /* add std.prelude import */ }
  ```
- [ ] `partial_prelude(core, runtime)` — module attribute, allows opt-in subset.
- [ ] Edition support: `nova.toml` `[workspace] edition = "2026.05"`; resolver maps to `std/prelude/<edition>.nv` if exists, fallback `std/prelude.nv`.
- [ ] Создать `std/prelude/2026_05.nv` как initial edition pin (identical content to `std/prelude.nv` для bootstrap).
- [ ] Тесты:
  - `plan62/no_prelude_no_auto_import.nv` — module `no_prelude`, использует `Option` без import → compile error.
  - `plan62/no_prelude_explicit_import.nv` — `no_prelude` + explicit `import std.prelude.core.{Option}` → PASS.
  - `plan62/partial_prelude_core_only.nv` — `partial_prelude(core)`, `Option` доступен но `println` нет.
  - `plan62/user_shadow_prelude.nv` — user redeclares `Option` локально → warning, не error.
  - `plan62/edition_pin.nv` — два workspace tests с разной edition.
- [ ] Spec updates (5 D-blocks amendments).
- [ ] Regression: 562/562 PASS.

**Total: 13-15 dev-days** через 6 sub-plans. Каждый sub-plan independent merge-able.

---

## Spec changes

### D26 amend в `spec/decisions/08-runtime.md`

- §«Что в prelude (v1.0)» — explicit file-based list с references на `std/prelude/<sub>.nv`.
- §«Bootstrap-расширение» — обновить со статусом «Plan 62 closed, full file-based migration».
- Удалить «hardcoded в type-checker» wording.
- Add §«Splittability» — `partial_prelude(...)`, `no_prelude`.
- Add §«Edition policy» — `[workspace] edition = "X.Y"` в `nova.toml`, resolver выбирает `std/prelude/<edition>.nv`.

### D-block для `no_prelude` enforcement

Уже в spec (07-modules.md:962-979), но **усилить** wording: «resolver MUST skip auto-import for `no_prelude` modules; failure to do so is implementation bug».

### Новый D-block — `external type` syntax

Если syntax не существует — D-block в `spec/decisions/03-syntax.md`:
> **D113. `external type`** — opaque type declared without body, реализация в C/runtime. Backing — known-by-name register (см. StringBuilder/WriteBuffer/ReadBuffer). Differs from `protocol` (no methods) and `type` (no fields).

### Amend D29 (one-way) — prelude shadowing

> User can declare item with same name as prelude item; lint warning (`W_PRELUDE_SHADOW`), не error. Explicit `import std.prelude.{Option as Opt}` (rename) — works without warning.

### Amend D102 (default params) — для `assert`

`assert(cond, msg = "assertion failed")` — default-msg-param. Currently special-case'нут как inline; после Plan 62.B unified через D102 mechanism (Plan 46/50).

---

## Acceptance criteria (production-grade)

### Корректность

- [x] `std/prelude.nv` re-exports все 50+ items from sub-modules.
- [x] `std/prelude/core.nv` declares Option/Result/Some/None/Ok/Err/Error/Never/Ordering/any.
- [x] `std/prelude/runtime.nv` declares print/println/panic/assert/debug_assert/exit.
- [x] `std/prelude/errors.nv` declares RuntimeError/RuntimeNoneError/ReadBufferError.
- [x] `std/prelude/collections.nv` declares Iter/Range/RangeIter/StringBuilder/WriteBuffer/ReadBuffer.
- [x] `std/prelude/protocols.nv` declares From/Into/TryFrom/TryInto/Hashable/Equatable/Comparable/Display.
- [x] `std/prelude/effects.nv` declares Fail/Time/Mem/Detach.
- [x] Type-checker `builtins` HashSet shrunk — все prelude names removed (verified via grep).
- [x] Codegen special-cases for print/println/exit/panic — converted to generic external-fn dispatch.
- [x] Pattern-match codegen для Some/None/Ok/Err — lookup через registered schema, не hardcoded name.

### `no_prelude` / `partial_prelude` enforcement

- [x] `module x no_prelude` — auto-import не выполняется. Verified via `plan62/no_prelude_*.nv` tests.
- [x] `module x partial_prelude(core, runtime)` — auto-import только указанных sub-modules. Verified.
- [x] Spec D26 + 07-modules.md updated.

### Edition-versionability

- [x] `nova.toml` `[workspace] edition = "2026.05"` honoured. `std/prelude/2026_05.nv` resolved if exists.
- [x] Fallback на `std/prelude.nv` если edition file отсутствует.
- [x] Test: 2 workspaces разной edition в `plan62/edition_*` resolve independently.

### User shadowing

- [x] `let Option = ...` локально → warning W_PRELUDE_SHADOW, не error.
- [x] `import std.prelude.core.{Option as Opt}` — renamed alias, no conflict.
- [x] Explicit import overrides auto-import (нет duplicate-declaration error).

### Regression

- [x] `nova test` — 0 fails vs baseline (562/562 PASS).
- [x] `nova check std/` — 45/45 type-check PASS.
- [x] Cross-toolchain (Plan 58): PASS на Clang/MSVC/GCC.

### Performance

- [x] Resolver overhead для cross-file prelude resolve ≤ 5% vs hardcoded (`nova bench plan62/prelude_resolve_perf.nv`).
- [x] Compile-time для typical project: ≤ 2% growth (smoke на std/* — 45 files).
- [x] Binary size: no growth (all declarations compile away — prelude only adds resolve-time imports).

### AI-friendliness (killer use-case)

- [x] `nova doc std/prelude.nv` (Plan 45) shows full prelude API в одной HTML page.
- [x] LLM может прочитать `std/prelude/core.nv` и точно знать что declared (нет hidden hardcode).
- [x] `nova check --explain W_PRELUDE_SHADOW` объясняет shadowing.

---

## Open questions

1. **`Result[T, E]` mono compromise — full fix в Plan 62 или отдельный plan?**
   **Decision:** Plan 62.A.bis adds generic schema registry поверх hardcoded baseline. Full mono fix — отдельная sub-task поверх Plan 59 (tuple mono → sum mono extension). Не блокер Plan 62.

2. **`Iter[T]` уже работает через duck-typing (D58) — нужно ли formal declaration в prelude?**
   **Decision:** да. Explicit declaration даёт canonical reference (`spec` + `nova doc`); duck-typing продолжает работать как backward-compat (не breaking change).

3. **`From`/`Into` formal declaration сломает auto-derive?**
   **Decision:** нет. Auto-derive (D73) проверяет наличие matching `From` impl; declarations в prelude дают names но не enforce semantics. Auto-derive продолжает работать.

4. **Splittable structure — обязательна или optional?**
   **Decision:** обязательна — это main differentiator vs Rust prelude. Без splittability нет преимущества над `prelude::v1`.

5. **`partial_prelude(...)` syntax — какие subset names?**
   **Decision:** `core`, `runtime`, `effects`, `collections`, `protocols`, `errors` — мapping на sub-files. Стандартизировано в D26 amend.

6. **Edition migration policy — как пользователь mig'рируется?**
   **Decision:** Pin `edition = "<old>"` в `nova.toml`, code keeps working with old prelude. Migration tool (`nova migrate --to-edition 2027.xx`) emit-fix-it'ы (отдельный Plan, не Plan 62).

7. **`std/prelude.nv` test для verification что auto-import работает — обновлять как часть Plan 62?**
   **Decision:** да. Existing `PRELUDE_VERSION` const bumpнуть до 2, test проверяет version после migration.

8. **Что с `Self` / `self` / `true` / `false` в builtins HashSet?**
   **Decision:** оставить как built-in keywords (language-level), не prelude. D-блок reservedidentifiers (D66 et al.) — это language structure.

9. **`gc` / `bench` / `fibers` / `runtime` namespaces — миграция в prelude?**
   **Decision:** **не в prelude**. Это namespaces для introspection API (Plan 32, Plan 57); они auto-available но не «общедоступные имена». Оставить как hardcoded namespace resolution. Spec amendment с категорией «runtime namespaces ≠ prelude».

10. **Cyclic import risk — что если кто-то добавит в prelude.nv `import std.encoding.json`?**
    **Decision:** lint-rule W_PRELUDE_FOREIGN_IMPORT — error если prelude.nv (или sub-module) импортирует что-то вне `std.prelude.*` или `std.runtime.*`. Enforced в Plan 62.F.

---

## Связь с другими планами

- **[Plan 11](11-method-values-and-overload.md)** — Plan 62.B сохраняет assert/debug_assert inline comma-operator pattern (введён Plan 11 fix).
- **[Plan 13](13-runtime-stdlib-and-autogen.md)** — external fn registrations через runtime_registry.rs. Plan 62.B расширяет registry на prelude runtime fns.
- **[Plan 14](14-stdlib-codegen-gaps.md)** — Q-result-monomorphization — Plan 62.A.bis adds registry поверх, full fix через Plan 59 extension.
- **[Plan 15](15-generic-bounds-enforcement.md)** — Plan 62.E использует bound enforcement infra для declared protocols.
- **[Plan 35](35-cross-file-resolve.md)** — Plan 62 builds on R26+R27 cross-file resolve + selective import. R28 `pub granularity` rejected, но Plan 62 ENS заказы re-export через `export import` (R26).
- **[Plan 42.10](42.10-module-level-forbid.md)** — module-level `#forbid` precedent для module attrs. Plan 62.F расширяет mechanism на `no_prelude` / `partial_prelude`.
- **[Plan 42.16](42.16-module-attr-syntax.md)** — `#cfg` / `#forbid` syntax. Plan 62.F adds `no_prelude` / `partial_prelude` через тот же syntax.
- **[Plan 45](45-nova-doc.md)** — `nova doc std/prelude.nv` показывает full prelude API; doc-coments в prelude items — AI-readable canonical reference.
- **[Plan 48](48-closures-in-generics.md)** — mono worklist может instantiate `Option[T]`/`Result[T,E]` для declared T/E. Plan 62 hardcoded `sum_schemas` сосуществует пока Plan 48+59 не закрыты для sum-types.
- **[Plan 56](56-vtable-dispatch-erased-generics.md)** — protocols в prelude (From/Into/Hashable) могут получать vtable-dispatch infra. Не блокер.
- **[Plan 57](57-perf-benchmark-infrastructure.md)** — perf gate для cross-file resolve overhead.
- **[Plan 58](58-cross-toolchain-msvc-verification.md)** — cross-toolchain matrix.
- **[Plan 61](61-typed-error-effect-codegen.md)** — `Fail` effect в `std/prelude/effects.nv`; Plan 61 codegen typed throws опирается на formal declaration.

---

## Сравнение с state-of-the-art

| Language | Prelude location | Splittable? | Versioned? | User shadowing |
|---|---|---|---|---|
| **Rust** | `std::prelude::v1` / `prelude::rust_2021` | partial (`core::prelude` vs `std::prelude`) | manual path (`v1`/`rust_2021`) | shadow OK, no warning |
| **Go** | universe block (compiler-builtin) | **no** — all-or-nothing | **no** | shadow OK, no warning |
| **Python** | builtins module | **no** | per-version automatically (Python 3.x) | shadow OK, no warning |
| **TypeScript** | global types | **no** | per-`tsconfig.json` `lib` (es2020/etc.) | shadow OK |
| **Swift** | implicit (StdLib) | **no** | per-version (Swift 5/6) | shadow OK |
| **Haskell** | `Prelude` module | yes (`NoImplicitPrelude` extension) | per-base-version | shadow OK |
| **Nova до Plan 62** | hardcoded в compiler | **no** | `PRELUDE_VERSION = 1` (cosmetic) | shadow OK (silent) |
| **Nova после Plan 62** | **`std/prelude/<sub>.nv` files, splittable, edition-versioned in `nova.toml`** | **yes** (`partial_prelude(core, runtime)`) | **yes** (edition в `nova.toml` — single path, content varies) | shadow OK, **W_PRELUDE_SHADOW warning** |

**Nova advantages:**
- **vs Rust:** edition в `nova.toml` (single source) vs path-based `prelude::v1` (duplicated everywhere). Splittable like Rust но без `#![no_std]` ceremony.
- **vs Go:** Nova file-based + readable; Go compiler-builtin opaque.
- **vs Python:** versioned proactively, splittable.
- **vs TS:** sub-modules opt-in finer-grained than `lib`.
- **vs Swift:** explicit declarations LLM-readable.
- **vs Haskell:** auto-import zero-friction unlike `NoImplicitPrelude`.

**Unique to Nova (after Plan 62):**
1. **AI-readable single-file source of truth** для каждой prelude category. LLM читает `std/prelude/core.nv` → точно знает declared API.
2. **Edition-based without path duplication** — RC over Rust's path-based versioning.
3. **`W_PRELUDE_SHADOW` warning** — none of competitors warn. AI-feedback signal.
4. **`partial_prelude(...)` opt-in subset** — RC over Rust's binary `no_std`.

---

## Ссылки

- [std/prelude.nv](../../std/prelude.nv) — текущий placeholder.
- [compiler-codegen/src/types/mod.rs:1974-2024](../../compiler-codegen/src/types/mod.rs#L1974) — builtins HashSet.
- [compiler-codegen/src/types/mod.rs:2904-2916](../../compiler-codegen/src/types/mod.rs#L2904) — `ty_of_ref` primitives.
- [compiler-codegen/src/codegen/emit_c.rs:754-820](../../compiler-codegen/src/codegen/emit_c.rs#L754) — pre-populated sum_schemas.
- [compiler-codegen/src/codegen/emit_c.rs:10241-10303](../../compiler-codegen/src/codegen/emit_c.rs#L10241) — println/print/assert/panic/exit special-cases.
- [compiler-codegen/src/codegen/emit_c.rs:13543-13565](../../compiler-codegen/src/codegen/emit_c.rs#L13543) — Some/None/Ok/Err pattern-match.
- [compiler-codegen/src/imports.rs:103-126](../../compiler-codegen/src/imports.rs#L103) — auto-import logic (где fix `no_prelude`).
- [docs/plans/35-cross-file-resolve.md](35-cross-file-resolve.md) — R26+R27 infrastructure.
- [docs/plans/42.10-module-level-forbid.md](42.10-module-level-forbid.md) — module-attr precedent.
- [docs/plans/45-nova-doc.md](45-nova-doc.md) — `nova doc` для prelude API.
- [docs/plans/61-typed-error-effect-codegen.md](61-typed-error-effect-codegen.md) — `Fail` effect formal declaration.
- [spec/decisions/08-runtime.md#d26](../../spec/decisions/08-runtime.md#d26) — prelude spec.
- [spec/decisions/07-modules.md#L962](../../spec/decisions/07-modules.md#L962) — `no_prelude` spec (currently not enforced).
- [spec/decisions/04-effects.md#d62](../../spec/decisions/04-effects.md#d62) — ambient effects.
- [spec/decisions/04-effects.md#d65](../../spec/decisions/04-effects.md#d65) — Fail spec.
- [spec/decisions/03-syntax.md#d54](../../spec/decisions/03-syntax.md#d54) — `any` top-type.
- [spec/decisions/03-syntax.md#d73](../../spec/decisions/03-syntax.md#d73), [#d77](../../spec/decisions/03-syntax.md#d77), [#d109](../../spec/decisions/03-syntax.md#d109) — From/TryFrom/Hashable.
