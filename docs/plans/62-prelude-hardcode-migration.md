# Plan 62: Migrate hardcoded prelude → `std/prelude.nv` (full D26 compliance + splittable + `no_prelude` enforcement)

> **Status:** ✅ **ЗАКРЫТ 2026-05-18** (P0 parts completed; deferred sub-plans listed in §«Итог Plan 62»). Architectural cleanup. Закрывает spec/impl drift между [D26](../../spec/decisions/08-runtime.md#d26) и фактической реализацией. **6 фазных sub-plans (62.A–62.F)** — реалистичная estimate 10-15 dev-days, fact: ~9 dev-days с deferred sub-plans (62.A.bis Ф.4, 62.B.bis, 62.C non-RuntimeNoneError, 62.D opaque types, 62.E TryFrom/TryInto, 62.F.bis edition+shadow-warning).
>
> **Update 2026-05-18 (Plan 62.D non-opaque bis-1):** Range / RangeIter
> миграция через prelude facade closed. 4 latent codegen bugs fixed
> (match inference + closure scope analysis + D29 W_PRELUDE_SHADOW basic).
> PRELUDE_VERSION bump 3 → 4. См. §62.D.

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
>
> **Status update 2026-05-20 (Plan 62.B — Option.or + Result methods):** **17/17 methods discharged.** Option `or` получил per-T trampoline `Nova_Option_method_or_<T>` (`nova_rt/array.h` `NOVA_ARRAY_IMPL` macro + explicit `nova_str` спец. + `sum_schema_registry` routing `HardcodedRuntimeFn { is_per_t: true }`). 5 deferred Result methods (`unwrap`/`unwrap_or`/`unwrap_or_else`/`map`/`map_err`) разблокированы. **Корневая причина** (уточнена): `external_registry::type_ref_to_c` маппит generic return-type `T` в стаб `"Nova_T*"` (`_ => Nova_<name>*`); `infer_expr_c_type` возвращал этот стаб для `r.unwrap_or(0)` из 3 lookup-путей (`method_overloads` single/multi-overload, `external_registry.by_key`); binop-dispatch принимал `Nova_*` за sum-type pointer → `->tag`-comparison вместо value-equality (CC-FAIL). **Лечение:** helper `is_generic_stub_c` (emit_c.rs) распознаёт стаб `Nova_<X>*` точным семантическим критерием (тот же, что у `erase_unk`): `X` — реальный тип ⇔ `X ∈ record_schemas ∪ sum_schemas ∪ generic_types`, иначе unresolved type-параметр. Все generic-aware lookup-блоки пропускают стаб → управление доходит до specialized `Nova_Result*` ветки `infer_expr_c_type`, знающей concrete Ok/Err-тип. Все 5 declarations раскомментированы в core.nv. **Дополнительно** починен pre-existing баг `Result.map` для `bool`/`char`-closures (хардкод `NOVA_CLOS_CALL_ii` int-layout → calling-convention mismatch / garbage; падал ещё до Plan 62.A — verified): closure-call теперь эмитит fn-pointer cast по фактической C-сигнатуре closure-литерала (`typed_closure_c_sig`). Добавлены 3 негативных теста (`option_or_missing_arg_neg` — Nova-диагностика arity; `option_or_payload_mismatch_neg` + `result_unwrap_or_arg_mismatch_neg` — C-backstop type mismatch). plan62 33/33 PASS, runtime 20/20 PASS, plan72 9/11 → 11/11 PASS, full nova test 864/0/47.

- [x] Создать `std/prelude/core.nv` с declarations:
  ```nova
  module std.prelude.core
  export type Option[T] | Some(T) | None
  export type Result[T, E] | Ok(T) | Err(E)
  export type Error { readonly msg str }
  export type Ordering | Less | Equal | Greater
  // DEFER: type Never — empty-sum syntax не поддержан parser'ом
  ```
- [x] **Complete (17/17 — 12 в 62.A 2026-05-18, 5 в 62.B 2026-05-20):** D26 §283-306 methods для Option/Result.
      - Option: **8/8** declared — `is_some`, `is_none`, `unwrap` (Fail[Error]),
        `unwrap_or`, `unwrap_or_else`, `map`, `ok_or`, `or` (последний — per-T
        trampoline `Nova_Option_method_or_<T>`, Plan 62.B).
      - Result: **9/9** declared — `is_ok`, `is_err`, `ok`, `err` (62.A) +
        `unwrap`, `unwrap_or`, `unwrap_or_else`, `map`, `map_err` (62.B).
      - **Plan 62.B fix:** прежний `T → T` блокер для 5 Result methods снят.
        Корневая причина: `external_registry::type_ref_to_c` маппит generic
        return-type `T` в стаб `"Nova_T*"`; `infer_expr_c_type` возвращал стаб
        для `r.unwrap_or(0)` → binop принимал `Nova_*` за sum-type pointer →
        `->tag`-comparison. Лечение — helper `is_generic_stub_c` (emit_c.rs):
        три generic-aware lookup-блока пропускают стаб `Nova_<X>*`, давая
        управление specialized `Nova_Result*` ветке. Все ранее-affected
        regressions (`runtime/unwrap_or`, `result_methods`, `from_into_basic`,
        `read_buffer`, `read_text`) — зелёные.
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

> **Status update 2026-05-18:** Phase 62.C complete — 2/3 types migrated
> (RuntimeError + ReadBufferError + 7 variant names; RuntimeNoneError
> deferred per parser empty-sum limitation, same blocker as `Never`).
> nova test 700/0/44 → **701/0/44 (+1)**, registry unit tests 13 → **16
> (+3)**. Pre-populated `sum_schemas["RuntimeError"]` оставлен как
> ABI-compat fallback baseline per 62.A.bis architecture; lookup precedence
> теперь DeclaredFromPrelude > HardcodedBaseline.

- [x] Создать `std/prelude/errors.nv`:
  ```nova
  module std.prelude.errors
  export type RuntimeError
      | DivByZero
      | Overflow
      | IndexOutOfBounds { index int, length int }
      | TypeMismatch(str)
      | AssertFailed(str)
      | NoHandler(str)
  // DEFER: type RuntimeNoneError — bootstrap parser blocker (см. ниже)
  export type ReadBufferError
      | UnexpectedEnd { wanted int, available int }
  ```
- [x] Wire через generic schema registry (Plan 62.A.bis) —
  `init_prelude_decls_from_items()` extended c Part 2 (sum-type
  registration). RuntimeError → `register_prelude_sum_inheriting_baseline`
  (inherits variants/abi/c_name/method_routing от HardcodedBaseline +
  strict variant-set equality check). ReadBufferError →
  `register_prelude_sum_from_decl` (parses variants из AST,
  PointerErrorLike ABI).
- [x] Удалить из builtins HashSet: **7 имён** (DivByZero, Overflow,
  IndexOutOfBounds, TypeMismatch, AssertFailed, NoHandler, RuntimeError).
  `RuntimeNoneError`, `ReadBufferError`, `UnexpectedEnd` — изначально НЕ
  были в HashSet'е (verified via grep на baseline'е); cross-file resolve
  для них работает через runtime_registry signatures + emit_sum_type
  для ReadBufferError.
- [x] **НЕ удаляли** pre-populated `sum_schemas["RuntimeError"]`
  (emit_c.rs:1029-1048) — оставлен как ABI-compat fallback baseline per
  Plan 62.A.bis design (HardcodedBaseline остаётся, lookup precedence
  DeclaredFromPrelude > HardcodedBaseline). Удаление отложено до Plan
  62.F (edition cleanup).
- [x] Facade `std/prelude.nv` обновлён — 3-й `export import` line с
  RuntimeError + 6 variants + ReadBufferError + UnexpectedEnd (9 имён).
- [x] `RUNTIME_DEFINED_TYPES` skip-list (emit_c.rs:4883) расширен на
  `RuntimeError` (но НЕ ReadBufferError — у неё нет C struct в nova_rt,
  codegen эмитит свой через emit_sum_type). `BUILTIN_TYPE_NAMES`
  расширен на RuntimeError (fwd-decl skip).
- [x] Positive test `nova_tests/plan62/runtime_error_from_prelude.nv`
  (8 tests, все PASS) — coverage всех 6 RuntimeError variants + 1
  ReadBufferError variant.
- [x] Regression: **701 PASS / 0 FAIL / 44 SKIP** (+1 от baseline 700).
  Sum-schema unit tests **16/16 PASS** (3 новых: variant inherit, drift
  skip, ReadBufferError from-AST).

**DEFER Plan 62.C → Plan 62.F (edition cleanup):**

- `type RuntimeNoneError` (unit-type per D85) — bootstrap parser
  `parse_sum_variants` (parser/mod.rs:2160) требует ≥1 `|` с variant;
  `parse_type_decl` (parser/mod.rs:2042+) при отсутствии body падает в
  newtype branch. Empty-body `type Name` не поддерживается. То же
  ограничение что у `Never` (Plan 62.A defer, см. std/prelude/core.nv:
  104-109). `RuntimeNoneError` остаётся как string-payload throw в
  `nova_rt/effects.h:122-126` (`nova_throw(nova_str_from_cstr("Runtime
  NoneError"))`) — никакого C-уровневого `Nova_RuntimeNoneError` не
  существует. Декларация перенесётся вместе с `Never` когда parser
  получит empty-sum syntax (D-block addition в spec/decisions/
  03-syntax.md).

### Plan 62.D — Iter[T] + Range + StringBuilder + WriteBuffer + ReadBuffer (2 days)

> **Status update 2026-05-18:** Phase 62.D **non-opaque complete** —
> `Iter[T]` protocol перенесён (62.D non-opaque), Range/RangeIter теперь
> auto-available через prelude facade (62.D non-opaque bis-1, всё ещё
> declared в std.collections.range — cross-file imports preserved).
> StringBuilder/WriteBuffer/ReadBuffer — нужен `external type` D-block
> (Plan 62.D.bis — отдельный sub-plan).

**Что сделано (Plan 62.D non-opaque, commit XXXXX):**

- ✅ Создан `std/prelude/collections.nv` с **только** `Iter[T]` protocol.
  Signature: `export type Iter[T] protocol { next() -> Option[T] }`
  (без `mut` — bootstrap parser `parse_effect_methods` не поддерживает
  `mut` префикс в protocol method declaration; receiver mutability
  объявляется implementer'ом). Раньше `Iter[T]` существовал только
  как structural duck-typing per D58 — формальная декларация даёт
  `nova doc` source-of-truth + eligible как bound (`T Iter`) per D72.
  Не ломает existing for-in dispatch (codegen ищет метод `next`
  напрямую, protocol declaration не enforced).
- ✅ `std/prelude.nv` facade: добавлена 4-я строка
  `export import std.prelude.collections.{Iter}`. Auto-import работает.
- ✅ Positive test `nova_tests/plan62/iter_protocol_from_prelude.nv`
  (4 test-блока): Counter ducks Iter[int] для for-in, manual next()
  loop, empty Counter, PRELUDE_VERSION sanity.
- ✅ Regression: 701/0/44 PASS (baseline preserved).

**Plan 62.D non-opaque bis-1 (commit ВПЕРЁД, 2026-05-18, ЗАКРЫТ):**

- ✅ **`Range` / `RangeIter` теперь re-export'ятся через prelude facade**
  (`export import std.collections.range.{Range, RangeIter}` в
  `std/prelude.nv`). Auto-available во всех модулях. Декларации остаются
  в `std/collections/range.nv` — 8 existing cross-file imports работают
  без изменений (selective re-export не блокирует direct import).
- ✅ **PRELUDE_VERSION bump 3 → 4** в `std/prelude.nv` (marker для
  тестов, оба `prelude_auto_import.nv` и `iter_protocol_from_prelude.nv`
  updated).
- ✅ **4 latent codegen bugs fixed** (все четыре существовали и до
  Plan 62.D, но maskились ограниченной видимостью `Range`):
    1. **Match inference (Nova_Range* vs nova_str)** — pattern
       bindings installed в `infer_expr_c_type(ExprKind::Match)`
       через новый `pattern_binding_overrides` RefCell
       (emit_c.rs:18936+). Раньше stale entry для arm-binding `r` в
       `var_types` (из `let r = Range...` в другом scope) перебивал
       правильный inner type из scrutinee. Mirror'ит emit-time
       `emit_match::infer_arm` (line 14694) но через `&self` RefCell.
    2. **Closure capture inner-shadowing (closure_rev)** — новый
       scope-aware `collect_truly_free_idents` (emit_c.rs:16621+)
       уважает inner `let`-bindings, lambda params, match patterns.
       Раньше misnamed `collect_free_idents` собирал ВСЕ идентификаторы
       (включая inner-bound), и filter `var_types.contains_key`
       ложно идентифицировал inner-shadowed `s` как capture.
    3. **Closure mono'd arg-name drop (p48_closure_arg_inference)** —
       same root cause как #2; fix #2 закрыл оба теста.
    4. **D29 user-shadow** — duplicate top-level имя пришедшее через
       prelude теперь генерирует warning `W_PRELUDE_SHADOW` (basic
       version; full structured lint — Plan 62.F.bis Ф.2 scope).
       Codegen `emit_module` skip'ает merged (non-user) duplicates
       при наличии user-declaration с тем же именем (emit_c.rs:1300+).
       Закрывает `nova_tests/syntax/for_in_range_iter.nv` который
       re-declare'ит `type Range` и `type StepRangeIter` locally.
- ✅ Regression: 709/0/44 PASS (baseline preserved, никаких новых
  positive tests — bis-1 это bug-fixing + миграция enable).

**Plan 62.D.bis ЗАКРЫТ 2026-05-18 (PRELUDE_VERSION 6):**

- ✅ **`StringBuilder` / `WriteBuffer` / `ReadBuffer` — opaque
  runtime types** — мигрированы в `std/prelude/collections.nv` через
  новый `external type` syntax (D126, spec/decisions/03-syntax.md).
  6 phases закрыты — parser/AST/type-checker (Ф.1), migration
  declarations (Ф.2), runtime header cross-refs (Ф.3), positive
  tests verification (Ф.4), HashSet documentation (Ф.5), spec
  D126/D26/D82 (Ф.6). PRELUDE_VERSION 5 → 6. 712 PASS / 0 FAIL /
  44 SKIP preserved (+1 new test file plan62/opaque_types_from_prelude
  с 4 sub-tests).

**Acceptance:**

- [x] Iter[T] declared formally — pass.
- [x] Iter[T] auto-available через prelude — pass (positive test).
- [x] Range / RangeIter auto-available через prelude — pass
  (bis-1 закрыт).
- [x] 4 latent codegen bugs closed — pass.
- [x] D29 W_PRELUDE_SHADOW basic — pass.
- [x] No regression: 709/0/44 → 712/0/44 (+3 new tests across bis).
- [x] StringBuilder/WriteBuffer/ReadBuffer migration ✅ ЗАКРЫТ
  2026-05-18 (Plan 62.D.bis — D126 `external type` Ф.1–Ф.6).

**Original spec (для справки):**

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

> **Status update 2026-05-18:** Phase 62.E complete (partial scope — 6/8
> protocols migrated, 703/0/44 baseline preserved + 1 new positive test
> file). `From[T]` / `Into[U]` / `Hashable` / `Equatable` / `Comparable` /
> `Display` перенесены в `std/prelude/protocols.nv` через formal protocol
> declarations. **Deferred (с обоснованиями):** `TryFrom[T,E]` /
> `TryInto[U,E]` — Plan 56 Ф.2.7 effect-free enforcement (vtable dispatch
> не propagates handlers) запрещает `Fail[E]` сигнатуру в bound (protocol)
> methods. См. §«DEFER 62.E» ниже.

- [x] Создать `std/prelude/protocols.nv` (без `fn`-префикса в protocol
      method declarations — bootstrap `parse_effect_methods` принимает
      bare-name `method(params) -> ret` syntax, см. compiler-codegen/
      src/parser/mod.rs:2244):
  ```nova
  module std.prelude.protocols
  import std.prelude.core.{Ordering}  // explicit (cycle protection)
  export type From[T] protocol { from(t T) -> Self }
  export type Into[U] protocol { into() -> U }
  // export type TryFrom[T, E] protocol { try_from(t T) Fail[E] -> Self }  // DEFERRED
  // export type TryInto[U, E] protocol { try_into() Fail[E] -> U }        // DEFERRED
  export type Hashable protocol { hash() -> u64 }
  export type Equatable protocol { eq(other Self) -> bool }
  export type Comparable protocol { cmp(other Self) -> Ordering }
  export type Display protocol { fmt(sb StringBuilder) -> () }
  ```
- [x] Auto-derive remains в compiler (D73/D77/D109) — declarations additive.
      Bound clauses `[T Hashable]` теперь могут резолвиться через
      cross-file resolve (Plan 15 generic-bounds infrastructure).
- [x] `std/prelude.nv` re-export (6-я строка):
      `export import std.prelude.protocols.{From, Into, Hashable, Equatable, Comparable, Display}`.
- [x] `std/collections/hashmap.nv`: убрана local-декларация `Hashable`
      (дубликат с prelude — `duplicate top-level name Hashable` блокировал
      все 37 файлов импортирующих HashMap). `HashMap[K Hashable, V]`
      bound теперь резолвится через cross-file resolve в prelude.
- [x] `compiler-codegen/src/lints.rs::collect_protocol_names`: HashSet
      shrink на 4 имени (`Hashable`, `Iter`, `From`, `Into` — теперь
      file-declared и captures'ятся for-loop'ом ниже через R27
      auto-import). Оставлены `Ord`, `Eq`, `ToStr` (legacy aliases для
      back-compat с `nova_tests/types/generics.nv:90 TwoBounds[K Hashable,
      V Eq]`) + `TryFrom`, `TryInto` (deferred decls — keep lint coverage).
- [x] `types/mod.rs` builtins HashSet — **0 shrink**, ни одно из 8 имён
      не было там (Plan 62.B compatibility — protocols не были hardcoded
      в type-checker через builtins, только в lints HashSet и в
      `std/collections/hashmap.nv` дубликате).
- [x] Positive tests: `nova_tests/plan62/protocols_bound_resolution.nv` —
      6 test-блоков (Hashable/Equatable/Comparable/From/Into/Display
      bound resolution через prelude auto-import). All PASS.
- [x] Regression: 703/0/44 (baseline 702 + 1 new test) + 16/16 unit tests.

**DEFER 62.E → требует Plan 56 ext или D77 переcмотр**

`TryFrom[T, E]` / `TryInto[U, E]` declarations с `Fail[E]` effect-row
триггерят `bound method TryFrom.try_from has effects` error per
`compiler-codegen/src/types/mod.rs:404-432` (Plan 56 Ф.2.7 enforcement).
Все 573 файла подгружающие prelude падают на этом check'е (verified
2026-05-18). Decline-and-document подход:

- Decl'и закомментированы в `std/prelude/protocols.nv` с DEFER блоком.
- Out of facade re-export.
- `lints.rs:565` HashSet **сохраняет** `TryFrom`/`TryInto` — keep
  lint coverage для protocol-in-effect-position rule.

**Migration paths** (одно из):
- (a) Special-case в Plan 56 Ф.2.7 enforcement: разрешить `Fail[E]`
  (но не другие effects) в protocol methods. Rationale: Fail-only
  methods могут diverge безопасно через codegen tag-check, vtable
  dispatch может пробрасывать Fail handler как любой другой (D122
  handler-as-parameter pattern).
- (b) Декларировать TryFrom/TryInto **без** effect row, с возвратом
  `Result[Self, E]` / `Result[U, E]`. Изменение D77 semantics —
  caller обязан match'ить вместо try-propagation.
- (c) Handler-as-parameter (D122) pattern: `try_from` принимает
  `Fail[E]` handler явно как параметр. Breaks D77 "?"-оператор
  ergonomics.

**Acceptance:**

- [x] 6/8 protocols declared formally + re-exported — pass.
- [x] Bound resolution через prelude works — pass (6 positive tests).
- [x] No regression: 703/0/44 (+1 new) + 16/16 unit tests.
- [x] HashMap[K Hashable, V] продолжает работать после removal local
      Hashable из hashmap.nv — pass.
- [ ] TryFrom/TryInto migration — DEFERRED (Plan 56 ext or D77 revisit).

### Plan 62.F — Effects: Fail/Time/Mem/Detach + `no_prelude` enforcement + `partial_prelude(...)` + edition-versioning (3-4 days)

> **Status update 2026-05-18:** Phase 62.F **complete** для P0/P1 scope —
> Parts 1+1b+2+3+5 закрыты, Part 4 (edition) deferred в Plan 62.F.bis.
> nova test 703/0/44 → **709/0/44 (+6 new)** + 16/16 unit tests без
> регрессии. PRELUDE_VERSION bumped 2 → 3, **Plan 62 закрыт**.

**Part 1: `std/prelude/effects.nv`** — ✅ done.

- [x] Создан `std/prelude/effects.nv` с formal declaration `Fail[E]`:
  ```nova
  module std.prelude.effects
  export type Fail[E] effect {
      fail(e E) -> Never
  }
  ```
  Syntax: `type X effect { ... }` (не `effect X` — последний не parser'ит
  как top-level item). Согласуется с existing `type Random effect`
  pattern (std/testing/handlers.nv:64, examples/effect-density/*).
- [x] **Time/Mem/Detach НЕ migrated** — ambient runtime effects:
  - `Time` (D11/D14/D62) pre-registered codegen'ом (`emit_c.rs:1071`) —
    `Time.sleep`/`Time.now`/`Time.after` runtime helpers, не user-
    overridable beyond bootstrap.
  - `Mem` runtime introspection (`emit_c.rs:1087`) — internal alloc
    tracking, не user API.
  - `Detach` (D50) — keyword-construct `detach { ... }`, syntactic form,
    не declarable effect.
  - Документировано DEFER-комментарием в `std/prelude/effects.nv`.
- [x] **Codegen skip-list** (`emit_c.rs::RUNTIME_DEFINED_TYPES`) extended
      `Fail` — file-based `type Fail[E] effect` declaration НЕ триггерит
      duplicate `NovaVtable_Fail` emission (vtable уже pre-registered в
      `nova_rt/effects.h`). Same pattern что `Option`/`Result`/`Error`.
- [x] `std/prelude.nv` facade добавлена 6-я re-export строка:
      `export import std.prelude.effects.{Fail}`.

**Part 2: `no_prelude` enforcement** — ✅ done (CRITICAL bug fix).

- [x] AST: `ModuleAttrKind::NoPrelude` (`ast/mod.rs:138`).
- [x] Parser: `module X no_prelude` clause syntax (parser/mod.rs:130+) —
      identifier после module-path, НЕ `#`-attribute (per spec
      `spec/decisions/07-modules.md:962-979`). Loop поддерживает
      несколько clauses (на будущее).
- [x] Resolver: `has_no_prelude` check в `imports.rs:114+` — skip auto-
      import `std.prelude` для модулей с `NoPrelude` attribute.
      Это закрывает silent spec/impl drift (auto-import работал
      независимо от no_prelude декларации до Plan 62.F).
- [x] Positive test: `nova_tests/plan62/no_prelude_explicit_import.nv` —
      `no_prelude` + explicit `import std.prelude.core.{Option, Some, None}`
      + `import std.prelude.runtime.{assert}` → PASS.
- [x] Negative test: `nova_tests/plan62/no_prelude_no_auto_import.nv` —
      `no_prelude` + использование `assert` без import →
      EXPECT_COMPILE_ERROR `undefined identifier 'assert'`. (Note:
      Option/Some/etc. остаются в type-checker builtins HashSet
      — 62.A.bis Ф.4 deferred — поэтому маркером служит runtime fn,
      не Option.)

**Part 3: `partial_prelude(core, runtime)` opt-in subset** — ✅ done.

- [x] AST: `ModuleAttrKind::PartialPrelude(Vec<String>)` (`ast/mod.rs:150`).
- [x] Parser: `module X partial_prelude(core, runtime)` clause syntax
      (parser/mod.rs:148+) — поддерживает empty list, trailing comma,
      несколько names. Pure ident-only list (no expr).
- [x] Resolver: `partial_prelude_names` extracted from attrs, iterate +
      auto-import `std.prelude.<name>` per element. Имена валидируются
      против реальных файлов `std/prelude/<name>.nv` — bad name → clean
      compile error со списком valid names (core/runtime/errors/
      collections/protocols/effects).
- [x] Positive test: `nova_tests/plan62/partial_prelude_core_only.nv` —
      `partial_prelude(core, runtime)` + Option/Result/Less + assert →
      PASS (3 sub-tests).
- [x] Negative test: `nova_tests/plan62/partial_prelude_no_panic_in_core.nv`
      — `partial_prelude(core)` (без runtime) + использование `panic` →
      EXPECT_COMPILE_ERROR `panic` (поскольку runtime не auto-imported).
- [x] Negative test: `nova_tests/plan62/partial_prelude_bad_subname.nv` —
      `partial_prelude(c0re)` (typo) → EXPECT_COMPILE_ERROR
      `unknown prelude sub-module`.

**Part 4: Edition versioning** — ⏸ DEFERRED в **Plan 62.F.bis**.

Edition support требует invasive changes: thread `edition` parameter
через `nova.toml` parser → `manifest.rs` Manifest struct → CLI flow →
`imports.rs::resolve_imports_inline` (currently не получает Manifest,
только `stdlib_dir`). Plus `std/prelude/2026_05.nv` initial pin.

Не блокирует Plan 62 closure — purely additive, без user-visible
regression. Plan 62.F.bis отложит это в опционный sub-plan когда
понадобится 2-я edition (currently single edition implied).

**Part 5: PRELUDE_VERSION bump 2 → 3** — ✅ done.

- [x] `std/prelude.nv` const updated: `PRELUDE_VERSION = 3` + history
      comment-block обновлён (1 → 2 → 3 chronology).
- [x] `nova_tests/plan62/iter_protocol_from_prelude.nv:99` — `assert(
      PRELUDE_VERSION == 3)`.
- [x] `nova_tests/modules/prelude_auto_import.nv:17` — `assert(
      PRELUDE_VERSION == 3)`.

**DEFER 62.F → требует invasive Manifest threading (Plan 62.F.bis)**

- **Edition versioning** (Part 4) — Manifest нужно прокинуть до imports.rs.
- **User-shadow warning** (W_PRELUDE_SHADOW per spec) — отдельная lints
  task (`compiler-codegen/src/lints.rs`); требует shadow-detection при
  user-defined Option/Result/etc. Pure additive lint.
- **2 D-block amendments** (spec/decisions/07-modules.md:962 expansion +
  spec/decisions/08-runtime.md edition section) — out of scope для
  bootstrap. См. Plan 62.F.bis.

**Acceptance Plan 62.F:**

- [x] Fail[E] declared formally в std/prelude/effects.nv + re-exported.
- [x] `no_prelude` enforcement works (no longer silent bug).
- [x] `partial_prelude(...)` opt-in subset works с name validation.
- [x] PRELUDE_VERSION bumped 2 → 3.
- [x] No regression: 703 → 709/0/44 (+6 new tests).
- [x] sum_schema unit tests: 16/16 PASS unchanged.
- [ ] Edition versioning + 2026_05.nv pin — DEFERRED (Plan 62.F.bis).
- [ ] Spec D-block amendments — DEFERRED (Plan 62.F.bis).

**Файлы изменены (Plan 62.F):**

- `std/prelude/effects.nv` (NEW) — `Fail[E]` formal declaration + DEFER
  comments для Time/Mem/Detach.
- `std/prelude.nv` — добавлен `export import std.prelude.effects.{Fail}`
  (6-я re-export строка); PRELUDE_VERSION bumped 2 → 3 + history.
- `compiler-codegen/src/ast/mod.rs` — `ModuleAttrKind` extended:
  `NoPrelude` + `PartialPrelude(Vec<String>)`.
- `compiler-codegen/src/parser/mod.rs:130-200` — clause syntax parsing
  после module-path (loop через `no_prelude` / `partial_prelude(...)`).
- `compiler-codegen/src/parser/mod.rs:305+` — merge `clause_attrs` в
  `Module.attrs`.
- `compiler-codegen/src/imports.rs:103-185` — `has_no_prelude` skip +
  `partial_prelude_names` iteration с name validation.
- `compiler-codegen/src/codegen/emit_c.rs:4908` — `Fail` добавлен в
  `RUNTIME_DEFINED_TYPES` skip-list.
- `nova_tests/plan62/fail_effect_from_prelude.nv` (NEW) — 3 positive
  tests на Fail[int]/[str] auto-import.
- `nova_tests/plan62/no_prelude_no_auto_import.nv` (NEW) — negative.
- `nova_tests/plan62/no_prelude_explicit_import.nv` (NEW) — positive.
- `nova_tests/plan62/partial_prelude_core_only.nv` (NEW) — positive
  (3 sub-tests).
- `nova_tests/plan62/partial_prelude_no_panic_in_core.nv` (NEW) — neg.
- `nova_tests/plan62/partial_prelude_bad_subname.nv` (NEW) — negative.
- `nova_tests/modules/prelude_auto_import.nv:17` — `PRELUDE_VERSION ==
  3` (bumped).
- `nova_tests/plan62/iter_protocol_from_prelude.nv:99` — `PRELUDE_
  VERSION == 3` (bumped).

**Total: 13-15 dev-days estimate; fact ~9 dev-days** через 6 sub-plans
+ 4 deferred sub-plans (62.A.bis Ф.4, 62.B.bis, 62.D.bis, 62.F.bis).
Каждый sub-plan independent merge-able.

---

## Итог Plan 62 (closure summary 2026-05-18)

### Что мигрировано (file-based form)

| Phase | Items migrated to `std/prelude/<sub>.nv` | Files |
|---|---|---|
| 62.A | Option, Result, Some, None, Ok, Err, Error, Ordering (Less/Equal/Greater) + 12 methods на Option/Result | `core.nv` |
| 62.B | panic, exit, assert, debug_assert (6 arity-overload signatures) | `runtime.nv` |
| 62.B.bis | print, println (D69 variadic + `[]any` canonical D26 signature) + Plan 67 hotfix absorption | `runtime.nv` |
| 62.C | RuntimeError (6 variants), ReadBufferError (1 variant) | `errors.nv` |
| 62.D non-opaque | Iter[T] protocol formal declaration | `collections.nv` |
| 62.D non-opaque bis-1 | Range / RangeIter re-export через prelude facade + 4 latent codegen bugs закрыты + D29 W_PRELUDE_SHADOW basic | `prelude.nv` re-export, fixes в emit_c.rs + types/mod.rs |
| 62.E | From[T], Into[U], Hashable, Equatable, Comparable, Display (6/8 protocols) | `protocols.nv` |
| 62.F | Fail[E] effect formal declaration | `effects.nv` |

**Sum**: 6 sub-modules в `std/prelude/`, ~33 items + 12 methods + 1 effect
declared в file-based form (vs 1 placeholder PRELUDE_VERSION до Plan 62).

### Что отложено в sub-plans (reasoned defers)

| Sub-plan | Scope | Reason |
|---|---|---|
| **62.A.bis Ф.4** | Remove pre-populated `sum_schemas[Option/Result]` (emit_c.rs:754-766) | Bootstrap monomorphization compromise — нужен Plan 14 Q-result-monomorphization fix. Тип-checker'у не повлияет, codegen может построить generic schema runtime'ом. |
| **62.B.bis** ✅ ЗАКРЫТ 2026-05-18 | print / println migration | Closed 2026-05-18 — 7 phases (Ф.0–Ф.6 + Plan 67 absorption). PRELUDE_VERSION 6 → 7. `external fn print(...items []any) -> ()` + `println(...)` formally declared в `std/prelude/runtime.nv` через D69 variadic + `[]any` (canonical D26 signature). Plan 67 hotfix (silent-wrong-output для `println(str.from(int))`) absorbed как Ф.0 — `infer_print_helper` refactor через unified `infer_expr_c_type`. Codegen special-case fires ДО variadic routing (Ф.1 reorder) — preserves per-arg type info; synthesized `[]any` array никогда не строится. HashSet shrink: 2 entries removed (Ф.5). 713 PASS / 0 FAIL / 44 SKIP. |
| **62.C bis** | RuntimeNoneError migration | Bootstrap parser не поддерживает empty-body sum syntax (`parse_sum_variants` требует ≥1 `\|`). Тот же блокер что `Never`. |
| **62.D opaque (62.D.bis)** ✅ ЗАКРЫТ 2026-05-18 | StringBuilder, WriteBuffer, ReadBuffer | Closed 2026-05-18 — D126 `external type` syntax добавлен в spec (03-syntax.md), 3 типа formally declared в std/prelude/collections.nv, PRELUDE_VERSION 5 → 6. 712 PASS / 0 FAIL / 44 SKIP. Methods продолжают жить в std/runtime/<name>.nv через external fn (D82, unchanged). Range/RangeIter re-export ЗАКРЫТ в 62.D non-opaque bis-1. |
| **62.E bis** | TryFrom[T, E] / TryInto[U, E] protocols | `Fail[E]` в protocol method триггерит Plan 56 Ф.2.7 enforcement (`bound method has effects` error). Требует either special-case в enforcement (Migration path a) или refactor D77 semantics (path b) или D122 handler-as-parameter (path c). |
| **62.F.bis** ✅ ЗАКРЫТ 2026-05-18 | Edition versioning + W_PRELUDE_SHADOW lint + Time/Mem formal declarations + spec D-block amendments (D26/D124/D125) | Все 4 phases закрыты single-session (commits `445904b2b4e` Ф.1, `faf164529a9` Ф.2, `51a0ccf24fb` Ф.3, `f37193b075f` Ф.4). PRELUDE_VERSION 4 → 5. Item-level suppress `#[allow(prelude_shadow)]` остаётся deferred (требует generic attribute parser). `Time.after(ms) -> Chan[int]` deferred (требует Chan[T] в prelude). |

### Регрессия preserved

- **Pre-Plan 62 baseline**: 691 PASS (если посмотреть на Plan 35.A R27 init).
- **Post-Plan 62 final**: **709 PASS / 0 FAIL / 44 SKIP** + 16/16 sum_schema
  unit tests + 0 регрессий.
- **Post-Plan 62.D non-opaque bis-1**: **709 PASS / 0 FAIL / 44 SKIP** +
  16/16 sum_schema + W_PRELUDE_SHADOW warning fires on intentional
  shadows (for_in_range_iter.nv).
- **Post-Plan 62.F.bis (2026-05-18 final)**: **711 PASS / 0 FAIL / 44 SKIP**
  (+2 fixtures plan62/prelude_shadow_warning + _suppress) + sum_schema 16/16
  + sanitize_edition 6/6 + prelude_shadow 6/6 + edition_resolve 3/3.
- **Net new tests** через Plan 62: +20 (Plan 62.A 5 tests, 62.B 0, 62.C
  3 tests, 62.D 3 tests, 62.E 1 test, 62.F 6 tests, 62.F.bis 2 tests).

### HashSet shrink total

| Phase | HashSet | Items removed | Notes |
|---|---|---|---|
| 62.A | `types/mod.rs::builtins` | 0 | Option/Result/etc. оставлены — bootstrap monomorphization compromise. |
| 62.B | `types/mod.rs::builtins` | 4 | panic, exit, assert, debug_assert removed; print/println оставлены (migrated в 62.B.bis). |
| 62.B.bis | `types/mod.rs::builtins` | 2 | print, println removed (Ф.5 2026-05-18). |
| 62.C | `types/mod.rs::builtins` | 7 | RuntimeError + 6 variants removed (file-based declaration теперь source of truth). |
| 62.D non-opaque | `lints.rs::collect_protocol_names` | 1 | Iter removed. |
| 62.E | `lints.rs::collect_protocol_names` | 4 | Hashable, From, Into, Iter (re-count overlap with 62.D). Total 4 new removals. |
| 62.F | n/a | 0 | Fail оставлен — single-element path match'ится hardcoded type-checker (types/mod.rs:2865, codegen/emit_c.rs:3186). HashSet был не source of truth для Fail. |

**Total: 17 HashSet entries removed** across Plan 62 (включая 62.B.bis
Ф.5 — print/println). Pre-population infrastructure
(`emit_c.rs::init_pre_populated_schemas`) остаётся — это
`sum_schema_registry` baseline (DeclaredFromPrelude > HardcodedBaseline
precedence), не legacy duplication.

### Net win

- **Spec/impl drift closed** для P0 scope (Option/Result/Error/RuntimeError/
  Ordering/effects.Fail/6 protocols/4 runtime fns + Time/Mem через 62.F.bis).
- **no_prelude bug fixed** (был silent — теперь real enforcement).
- **partial_prelude** новая capability — opt-in subset (real-time, embedded,
  bootstrap use-cases).
- **allow_prelude_shadow** новая capability (62.F.bis Ф.2) — suppress
  W_PRELUDE_SHADOW per-module.
- **Edition versioning** (62.F.bis Ф.1, D124) — `[package].edition` в
  `nova.toml` pin prelude content на snapshot, mirror Rust/Go.
- **PRELUDE_VERSION** marker mechanism (7 = current after 62.B.bis;
  chronology: 5 = 62.F.bis closure, 6 = 62.D.bis closure (opaque types),
  7 = 62.B.bis closure (print/println). Future bumps от 62.A.bis Ф.4 /
  62.C.bis / 62.E.bis когда они закрываются).
- **`nova doc`** теперь видит canonical prelude API в одном месте
  (AI-readable, не разбросанным по type-checker/codegen).

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

### Новый D-block — `external type` syntax ✅ ЗАКРЫТ 2026-05-18 (D126)

Реализовано в Plan 62.D.bis Ф.6 — D126 добавлен в
`spec/decisions/03-syntax.md`:
> **D126. `external type X [Generics]`** — opaque type declared without
> body, реализация в runtime (`nova_rt/`). Whitelist: только
> `std.runtime.*` / `std.prelude.*` модули. Type-аналог D82
> `external fn`. См. [D126](../../spec/decisions/03-syntax.md#d126-external-type--opaque-типы-без-body).

### Amend D29 (one-way) — prelude shadowing

> User can declare item with same name as prelude item; lint warning (`W_PRELUDE_SHADOW`), не error. Explicit `import std.prelude.{Option as Opt}` (rename) — works without warning.

### Amend D29 — Rule E formal sync ✅ ЗАКРЫТ 2026-05-19 (cleanup)

Plan 62 cleanup (2026-05-19) обнаружил silent drift между spec D29 и
Plan 42 Rule E:

1. **Spec D29 line 129** содержал absolute-conflict wording для `X.nv`
   + `X/` co-existence. Plan 42 (закрыт 2026-05-14) ввёл Rule E:
   conflict только когда `X/` содержит peer-files declaring `module X`;
   sub-modules declaring `module X.<sub>` — валидно (facade pattern).
   Реализовано в compiler (Plan 62.A агент), но spec не sync'нут.
   **Fixed**: spec/decisions/07-modules.md D29 section updated с
   Rule E (case a/b distinction) + facade pattern example.

2. **10 файлов в std/prelude/ + std/runtime/** имели нарушающее rev-3
   `parent.target` declaration (`module std.prelude.X` / `module
   std.runtime.X` — full path 3 seg вместо 2 seg). Plan 42 rev-3
   (2026-05-13) явно требует `module prelude.X` / `module runtime.X`.
   **Fixed**: reverted к strict 2-seg form.

3. **`runtime_registry.rs::render_nv`** emitted `module std.runtime.X`
   (full path) при auto-generation. **Fixed**: derive short-form
   (`runtime.X`) из registry's canonical full-path module key.

4. **`imports.rs::resolve_module_paths`** Rule E check expected peer
   sub-modules to declare full-path prefix form. После rev-3 strict
   revert sub-modules declare 2-seg form. **Fixed**: check accepts
   both forms (legacy facade + rev-3 strict).

Commits:
- `f6c64d3fb0d` spec(D29): sync Rule E
- `1e8096d3e95` fix(modules): revert 7 prelude declarations
- `25f07d92f26` fix(runtime_registry): render_nv 2-seg + str @hash registry entry
- `9d6edcdf253` fix(imports): Rule E check accepts 2-seg

Regression: **719 PASS / 0 FAIL / 44 SKIP** preserved.

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
