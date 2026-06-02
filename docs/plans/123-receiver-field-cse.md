// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 123 — Method-local field load optimization (umbrella)

> **Создан 2026-06-01.**
> **Status:** 🆕 PLANNED — roadmap-индекс. Декомпозирован на 7
> sub-plan'ов (123.1-123.7) per «split» в §5. Каждый sub-plan
> independently shippable; release-train V1→V7 incremental.
> **Приоритет:** P1 (V1 = 123.1 — codegen perf для всех Nova-методов;
> особенно ценно для hot-path кодеков ReadBuffer/WriteBuffer/
> StringBuilder/HashMap iter). V2-V7 — incremental wins.
> **Оценка (umbrella):** ~12-18 dev-day across 7 sub-plans.
> **Зависимости:** —. V2-V7 имеют specific gates (см. per-sub-plan).
> **D-блоки:** **D217-D219 NEW** (декомпозированы по слою
> оптимизации: D217 core CSE, D218 LICM, D219 pure-call caching).
> **Worktree convention:** `nova-p123` (umbrella); sub-plan'ы могут
> spawn'нуть own worktrees при необходимости (e.g. `nova-p123-3` для
> Plan 123.3 если LICM landed первым).
> **Recommended model:** **Opus 4.7 + Thinking ON** для всех
> sub-plan'ов — compiler analysis design-heavy, semantic equivalence
> guarantees, alias / escape edge cases.

---

## 1. Контекст и мотивация

### 1.1 Проблема

Codegen Nova эмитит **literal `self->X`** на каждый `@X`-доступ в
method body. Hot-path methods (ReadBuffer/WriteBuffer/StringBuilder
decoders/encoders, HashMap iter, str/[]T methods) accessing
`@field` 5-15 раз → redundant pointer dereferences в `.c` output.

Пример: `ReadBuffer.try_read_u32_le` — success-path ~7-8 dereferences
через `self->`, хотя семантически читаются 2 stable values (`data`
ptr + `pos` int).

### 1.2 Почему C-оптимизатор не спасает

1. **`-O0` (debug build):** zero CSE — каждый `self->X` literal load.
2. **`-O1`:** partial CSE, но conservative для mut fields.
3. **`-O2`+:** теоретически делает CSE через **NoAlias analysis** —
   но для Nova's emitted code:
   - Self pointer не annotated `restrict` → conservative aliasing.
   - Mut field write (`self->pos = ...`) blocks CSE across it
     (compiler не знает что `self->data` aliasing-disjoint).
   - Inlining limits (vtable dispatch / large methods) ограничивают
     C-level CSE.
4. **Plus:** даже под `-O2` clang/gcc behavior **различается** —
   non-deterministic perf characteristic.

### 1.3 Solution philosophy

**Compile-time транспарентная оптимизация** на уровне Nova-codegen,
независимая от C-compiler quality. User пишет идиоматичный код
с `@field` повсюду — codegen автоматически кеширует.

**Production-grade требования:**
- Semantic equivalence guarantee (formal property)
- Cross-platform deterministic (Windows MSVC / Linux clang / macOS clang)
- Bounded compile-time overhead (<5% per fn)
- Bounded stack-frame growth (cache locals ≤ N per fn)
- Escape hatch для diagnosis (`--field-cache-threshold=0`)
- Debug-info maps generated locals обратно к source positions

---

## 2. Comparative analysis vs Go/Rust/TS/Kotlin/Java

### 2.1 Capability matrix

| Capability | Go | Rust | TS/V8 | Kotlin/JVM | Java/HotSpot | **Nova (Plan 123 V7)** |
|---|---|---|---|---|---|---|
| Field CSE straight-line | ✅ SSA | ✅ MIR+LLVM | ✅ JIT | ✅ C2 | ✅ C2 | ✅ **AST-pass** (V1) |
| Mut field write-region | ✅ | ✅ | ⚠️ conservative | ✅ | ✅ | ✅ **V1** |
| LICM receiver fields | ✅ | ✅ | ✅ TurboFan | ✅ | ✅ | ✅ **V2 (123.2)** |
| Pure call result caching | ⚠️ limited | ⚠️ `#[inline]`+hints | ✅ JIT inline | ⚠️ annotations | ⚠️ annotations | ✅ **Effect-aware, V3 (123.3)** |
| Chain `a.b.c` cache | ⚠️ partial | ✅ NoAlias | ✅ JIT | ✅ | ✅ | ✅ **V4 (123.4)** |
| Diagnostic visibility | gcflags | `--emit=mir-cfg` | DevTools | JIT log | JIT log | ✅ **LSP code-lens, V5 (123.5)** |
| Escape hatch | `-gcflags=-N` | `-C opt-level=0` | n/a | `-XX:-DoEscapeAnalysis` | same | ✅ `--field-cache-threshold=0` |
| Cross-call IPA | ⚠️ limited | ✅ LTO | ✅ JIT | ✅ C2 | ✅ C2 | ✅ **V7 (123.7)** |
| Determinism cross-platform | ✅ | ✅ | ❌ JIT-variable | ❌ JIT-variable | ❌ JIT-variable | ✅ **AOT, deterministic** |
| Effect-aware optimization | ❌ no effect system | ❌ no effect system | ❌ | ❌ | ❌ | ✅ **D2 Nova edge** |
| `consume` linearity hint | ❌ | ⚠️ borrow checker proxy | ❌ | ❌ | ❌ | ✅ **D131 Nova edge** |

### 2.2 Nova edges (почему мы можем лучше)

Nova имеет **stronger static guarantees** чем все 5 эталонов:

1. **`ro` fields = TRUE immutable.** Rust имеет `&T` но с `Cell`/
   `RefCell` interior mutability — borrow checker conservative.
   Nova `ro` поле БЕЗ interior mutability cheat → unconditional
   cache across entire method body, безопасно.
2. **`consume` linearity (D131):** consume types — exclusive
   ownership, NO aliasing возможно → aggressive caching без alias
   analysis.
3. **Effect system (D2) explicit:** `perform Effect.X` — единственный
   способ side effect. Nova **знает** где callback может re-enter →
   precise invalidation points. Go/Rust/TS/Kotlin/Java должны
   conservative invalidate на every external call.
4. **No reflection, no dynamic field access:** all field reads
   статичны. JIT-языки спасает inline cache, но AOT cleaner.
5. **Generic mono (Plan 59 + 59.1):** concrete types known at
   codegen → cache type корректен per instantiation, no boxing.
6. **No hidden globals:** explicit effect handlers, no global state
   to invalidate caches (Go has `runtime.GC()` etc что может
   surprise; Java has finalizers).

### 2.3 Где другие лучше нас

- **Rust LLVM** делает global value numbering на larger basic blocks
  чем AST-pass можеть. V7 (IPA + LTO-style) — наш ответ.
- **TS/V8** имеет profile-guided JIT — adapts to runtime patterns.
  Nova AOT — alternative: PGO infrastructure через Plan 10 (PGO
  integration), future composition.
- **JVM HotSpot** has decades of mature OOP-optimization. Nova V1
  catch-up; V7 closes gap.

**Cumulative result для Nova V7:** match-or-exceed на 9 из 11
capabilities, миss только 2 (LLVM-grade GVN + profile-guided —
both addressable через other plans). Plus Nova-only 2 capabilities
(effect-aware + consume linearity).

---

## 3. Umbrella design (V1-V7 progression)

### 3.1 Release train

| Version | Sub-plans | Что | Релиз timing |
|---|---|---|---|
| **V1** | **123.1** | Core CSE: `ro` unconditional + `mut` straight-line | 0.1 ship-ready (foundational) |
| **V2** | + 123.2 | LICM: hoist field loads из loops | 0.1.1 |
| **V3** | + 123.3 | Pure call result caching (effect-aware) | 0.2 |
| **V4** | + 123.4 | Chain caching `@a.b.c` | 0.2 |
| **V5** | + 123.5 | LSP code-lens + diagnostic mode | 0.2+ (gated Plan 104.x) |
| **V6** | + 123.6 | Production telemetry + rollout | 0.2+ |
| **V7** | + 123.7 | Inter-procedural analysis (IPA) | 0.3+ |

Каждый Vn ↔ один sub-plan. Independently shippable — release не
ждёт следующий.

### 3.2 Architectural foundation

**AST-rewrite pass approach** (DECISION-A в Plan 123.1):
- Pass инвокируется ПОСЛЕ type-check (нужны типы для ro vs mut
  classification + concrete types для cache locals)
- ПЕРЕД codegen (downstream emit_c.rs unchanged)
- Pure transformation (input AST → output AST с insertions/replaces)
- Span preserved через node cloning
- Unit-testable как pure function

**Common infrastructure** (built в Plan 123.1, reused 123.2-123.7):
- `compiler-codegen/src/passes/field_cache/`:
  - `mod.rs` — top-level pass dispatcher.
  - `analysis.rs` — read/write counting, write-region detection,
    closure capture detection, perform invalidation points.
  - `rewrite.rs` — AST rewriter: insert prefix declarations,
    replace reads с cached idents.
  - `licm.rs` (V2) — loop-invariant code motion.
  - `pure_cache.rs` (V3) — pure-method result caching.
  - `chain.rs` (V4) — nested chain caching.
  - `diag.rs` (V5) — debug emit, code-lens metadata.
- `compiler-codegen/src/passes/mod.rs` — pass pipeline integration.

### 3.3 Cross-cutting requirements (применимо ко всем sub-plans)

1. **Semantic equivalence guarantee:** для каждой трансформации
   — formal property «observable behavior identical to source».
   Verified через:
   - Unit tests на AST level (input → expected output)
   - Codegen-level diff verification (.c before/after)
   - Semantic equivalence test (.nv programs run identically)
   - Property-based tests (random AST → equivalence check)
2. **Cross-platform determinism:** Windows MSVC / Linux clang /
   macOS clang — same input AST → same output AST → same .c
   (modulo platform-specific runtime).
3. **Bounded overhead:**
   - Compile-time: <5% per fn (measured через `nova bench`
     compile suite).
   - Stack-frame growth: ≤ N cache locals per fn (default N=8,
     tunable via `--field-cache-max=N`).
   - Binary size: ≤2% bloat (debug-info adds entries; release
     binaries shouldn't grow due to register allocation).
4. **Escape hatch:** `--field-cache-threshold=0` отключает feature
   полностью. Production users могут disable per-build если
   обнаружат regression.
5. **Debug-info:** generated locals мапятся обратно к source
   `@field` positions (DWARF / PDB) — debugger показывает meaningful
   variable names.
6. **Edition compat:** feature можно tie к edition (Plan 62.F.bis)
   — old-edition code не получает оптимизацию (safe fallback).
   V1 — enable unconditionally (no edition gate, semantic
   equivalence guarantee достаточная).
7. **Telemetry:** codegen pass emit'ит metric «N methods affected,
   M caches inserted per method (median/p99)» — для production
   monitoring (Plan 123.6).

---

## 4. Sub-plans 123.1-123.7

### 4.1 Plan 123.1 — Core CSE (V1 foundational)

**Scope:** `ro` field unconditional cache + `mut` field straight-
line write-region analysis.

**Status:** 🆕 PLANNED. Worktree: `nova-p123` (shared umbrella).
Доcument: separate file `docs/plans/123.1-core-cse.md` (TBD при
старте Ф.0).

**Phases (Ф.0-Ф.6):**
- Ф.0 Investigation + DECISION-A/B/C/D finalization.
- Ф.1 AST pass infrastructure + `ro` field caching.
- Ф.2 `mut` field straight-line region analysis.
- Ф.3 Edge cases: closures, `perform`, vtable, generic mono.
- Ф.4 Positive (≥10) + negative (≥5) tests + property tests.
- Ф.5 Spec D217 NEW + README index.
- Ф.6 Closure logs.

**Эстимат:** ~2-3 dev-day.

**Acceptance (A1-A10):** см. §6.1.

---

### 4.2 Plan 123.2 — LICM (Loop-Invariant Code Motion)

**Scope:** hoist `@field` reads ИЗ loops в pre-loop когда field не
modified в loop body. Параллель Rust/Go SSA LICM, но на AST-pass
уровне.

**Status:** 🆕 PLANNED. Gated на Plan 123.1 ✅.

**Example:**
```nova
// Before LICM:
mut sum = 0
for i in 0..n {
    sum = sum + @data[i] as int    // @data load каждую итерацию
}

// After LICM:
ro _at_data = @data                // hoisted ONCE
mut sum = 0
for i in 0..n {
    sum = sum + _at_data[i] as int  // cached read
}
```

**Hot-path выигрыш:** array iteration loops (`@items`, `@buffer`)
типичный N=1000+ итераций → 999 redundant pointer derefs устранены.

**Phases (Ф.0-Ф.6):**
- Ф.0 Loop pattern audit в std/ + nova_tests/.
- Ф.1 Loop AST visitor: detect `for`/`while`/`loop` + collect
  body `@field` accesses.
- Ф.2 Invariance analysis: field NOT modified в body → hoist.
  Handles nested loops, `break`/`continue`, early returns.
- Ф.3 Edge cases: loop body с `perform` (cache invalidate per
  iteration boundary), spawn (skip — concurrent access).
- Ф.4 Tests (10+ positive с iteration patterns, 5+ negative
  с in-loop mutation).
- Ф.5 Spec D218 NEW (LICM semantics for receiver fields).
- Ф.6 Closure logs.

**Эстимат:** ~2 dev-day.

**Acceptance:** см. §6.2.

---

### 4.3 Plan 123.3 — Pure call result caching (effect-aware, Nova edge)

**Scope:** `@field.method()` где `method` annotated `#pure` (D120
pure views) — результат cached per scope. Nova-unique feature
leveraging effect system.

**Status:** 🆕 PLANNED. Gated на Plan 123.1 ✅ + D120 #pure
infrastructure ✅ (Plan 100.x landed) + Effect system.

**Example:**
```nova
// @data.len() — pure, deterministic.
// Without caching: 3 calls.
if @data.len() < 4 { return Err(...) }
ro n = @data.len()
ro tail = @data.len() - p

// With Plan 123.3:
ro _at_data_len = @data.len()    // cached (pure, effect-free)
if _at_data_len < 4 { return Err(...) }
ro n = _at_data_len
ro tail = _at_data_len - p
```

**Safety:** только методы annotated `#pure` (D120) — guarantee'ит
no side effects, no effect performs, no mut state read.

**Phases (Ф.0-Ф.6):**
- Ф.0 D120 `#pure` audit: которые stdlib methods pure'ны?
  Default: `.len()`, `.is_empty()`, `.capacity()` candidates.
- Ф.1 Pass detects `@field.<pure_method>()` patterns; cache
  results per scope.
- Ф.2 Invalidation: pure call cached UNTIL `@field` mutation
  ИЛИ effect handler boundary.
- Ф.3 Edge cases: pure method с args (cache keyed by args),
  generic pure methods (mono'd cache).
- Ф.4 Tests с stdlib pure methods (`.len()`/`.is_empty()` cases).
- Ф.5 Spec D219 NEW (pure-call result caching semantics +
  D120 cross-ref).
- Ф.6 Closure logs.

**Эстимат:** ~2 dev-day.

**Acceptance:** см. §6.3.

---

### 4.4 Plan 123.4 — Chain caching `@a.b.c`

**Scope:** nested chain `@field.subfield.subsubfield` accessed N+
раз → cache intermediate paths.

**Status:** 🆕 PLANNED. Gated на Plan 123.1 ✅.

**Example:**
```nova
// Cumulative @parent.inner.cfg accesses → cache chain.
ro _at_parent = @parent
ro _at_parent_inner = _at_parent.inner
ro _at_parent_inner_cfg = _at_parent_inner.cfg
// reads: _at_parent_inner_cfg.flags, _at_parent_inner_cfg.limit, ...
```

**Heuristic:** cache subpath если accessed ≥2 раз; nested level
limit ≤4 (avoid stack bloat).

**Phases (Ф.0-Ф.6):**
- Ф.0 Pattern audit (which std/ методы делают chain access).
- Ф.1 Chain detection + recursive caching.
- Ф.2 Mutation invalidation cross-chain.
- Ф.3 Edge cases.
- Ф.4 Tests.
- Ф.5 Spec D217 amend (chain extension).
- Ф.6 Closure logs.

**Эстимат:** ~1-2 dev-day.

**Acceptance:** см. §6.4.

---

### 4.5 Plan 123.5 — LSP code-lens + diagnostic mode

**Scope:** developer visibility:
- LSP code-lens над method header показывает «N caches inserted»
- Hover на `@field` показывает «cached as `_at_field` from line X»
- `nova check --explain-cache` CLI flag — emit diagnostic per method

**Status:** 🆕 PLANNED. Gated на Plan 123.1 ✅ + Plan 104.x LSP ✅.

**Phases (Ф.0-Ф.6):**
- Ф.0 LSP infrastructure review (Plan 104.x).
- Ф.1 Code-lens provider implementation.
- Ф.2 Hover enhancement.
- Ф.3 CLI flag.
- Ф.4 Tests (LSP integration suite).
- Ф.5 Doc (user-facing usage examples).
- Ф.6 Closure logs.

**Эстимат:** ~1-2 dev-day.

**Acceptance:** см. §6.5.

---

### 4.6 Plan 123.6 — Production rollout + telemetry

**Scope:**
- Telemetry: codegen pass emit metric «N methods affected,
  M caches/method (median/p99)»
- `nova check --telemetry-cache` aggregates per-project stats
- Migration guide (edition gating если выявится regression)
- Performance regression tests in CI (Plan 57 bench harness)

**Status:** 🆕 PLANNED. Gated на Plan 123.1 ✅.

**Phases (Ф.0-Ф.6):**
- Ф.0 Telemetry infrastructure design.
- Ф.1 Codegen pass instrumentation.
- Ф.2 CLI flag + aggregator.
- Ф.3 CI integration (Plan 57 perf bench gates).
- Ф.4 Migration guide writing.
- Ф.5 Doc.
- Ф.6 Closure logs.

**Эстимат:** ~0.5-1 dev-day.

**Acceptance:** см. §6.6.

---

### 4.7 Plan 123.7 — V2 inter-procedural analysis (IPA)

**Scope:** cross-method analysis. Если method `f` calls method `g`,
и `g` annotated `#nofield_mut(<list>)`, then `f`'s caches for those
fields can survive `g`-call.

**Status:** 🆕 PLANNED. Long-term. Gated на ALL above + potentially
Plan 100.x effect-surface infrastructure.

**Approach:**
- Effect-surface inference (Plan 03.4 D140) даёт `f`'s effect
  signature → can determine field-mutation effects
- Per-method «field-write-set» annotation (auto-inferred OR explicit)
- Caches survive calls where caller's cached field ∉ callee's
  write-set

**Эстимат:** ~3-5 dev-day. Long-term.

**Acceptance:** см. §6.7.

---

## 5. Декомпозиция (split rationale)

Per project conventions (Plan 33.x, 91.x, 100.x, 103.x) — umbrella +
sub-plans patternой. Каждый sub-plan:
- Self-contained scope
- Independently shippable
- Own Ф.0-Ф.6 cycle
- Own acceptance criteria
- Own D-block (D217 / D218 / D219)
- Own closure logs entry

**Sub-plan dependency graph:**

```
                123.1 (V1 core)
                 │
       ┌─────────┼─────────┐
       │         │         │
     123.2     123.3     123.4
    (LICM)   (#pure)   (chain)
       │         │         │
       └────┬────┘         │
            │              │
          123.5 ←──────────┘
        (LSP/diag)
            │
          123.6
        (telemetry)
            │
          123.7
          (IPA)
```

123.1 — gate для всех остальных. 123.2-123.4 parallel-able. 123.5
gated на 123.1+104.x. 123.6 gated на 123.1. 123.7 gated на all
previous.

---

## 6. Acceptance criteria (per sub-plan)

### 6.1 Plan 123.1 acceptance (A1.1-A1.10)

- **A1.1** `ro` field accessed 2+ раз → emitted `_at_<field>`
  local prefix; subsequent reads через cache.
- **A1.2** `mut` field accessed 2+ раз в straight-line region
  → emitted cache; write invalidates / re-caches.
- **A1.3** Closure capturing `@field` → skip cache (conservative).
- **A1.4** `perform Effect.X` → invalidate all caches at boundary;
  re-cache after если accessed снова.
- **A1.5** Escape hatch `--field-cache-threshold=0` отключает
  feature полностью.
- **A1.6** ReadBuffer.try_read_u32_le before/after .c diff
  показывает reduction `self->data->...` from 5 to 1 + cache
  prefix.
- **A1.7** Regression — full nova test 0 new FAIL (baseline vs
  Plan 123.1 ON).
- **A1.8** Spec D217 NEW + README spec entry — landed.
- **A1.9** plan123_1 fixtures ≥15 (10+ positive, 5+ negative,
  3+ property tests) ALL PASS.
- **A1.10** Compile-time overhead <5% (measured via `nova bench`
  compile suite).

### 6.2 Plan 123.2 acceptance (A2.1-A2.8)

- **A2.1** `@field` read в loop body NOT modified → hoisted
  pre-loop.
- **A2.2** `@field = X` внутри loop body → no hoist (correctness).
- **A2.3** Nested loops: hoist на ближайший loop boundary без
  mutation.
- **A2.4** `break`/`continue`/early `return` не нарушают correctness.
- **A2.5** Perf bench: hot iteration loop (`for i in 0..N { @data[i] }`)
  показывает ≥20% improvement в `-O0` build, ≥5% в `-O2`.
- **A2.6** Regression — full nova test 0 new FAIL.
- **A2.7** Spec D218 NEW — landed.
- **A2.8** plan123_2 fixtures ≥10 (7+ positive, 3+ negative) PASS.

### 6.3 Plan 123.3 acceptance (A3.1-A3.8)

- **A3.1** `@field.<pure_method>()` × N → cached single call.
- **A3.2** `#pure` annotation respected (non-pure methods skip).
- **A3.3** Mutation of `@field` invalidates cached pure-call
  result.
- **A3.4** Effect handler boundary invalidates.
- **A3.5** Pure call с args — cache keyed by (method, args).
- **A3.6** Regression — 0 new FAIL.
- **A3.7** Spec D219 NEW — landed.
- **A3.8** plan123_3 fixtures ≥10 PASS.

### 6.4 Plan 123.4 acceptance (A4.1-A4.6)

- **A4.1** `@a.b.c` accessed 2+ раз → chain cached.
- **A4.2** Nested depth limit ≤4 (default).
- **A4.3** Mutation invalidates entire chain.
- **A4.4** Regression — 0 new FAIL.
- **A4.5** Spec D217 amended (chain extension).
- **A4.6** plan123_4 fixtures ≥8 PASS.

### 6.5 Plan 123.5 acceptance (A5.1-A5.6)

- **A5.1** LSP code-lens над method header показывает «N caches»
  count.
- **A5.2** Hover на `@field` показывает cache info.
- **A5.3** CLI flag `--explain-cache` emit'ит diagnostic.
- **A5.4** Regression — 0 new FAIL.
- **A5.5** LSP integration suite passes.
- **A5.6** User-facing doc написан.

### 6.6 Plan 123.6 acceptance (A6.1-A6.5)

- **A6.1** Telemetry emitted (% affected, median/p99 caches).
- **A6.2** Aggregator CLI works.
- **A6.3** CI perf regression gates wired.
- **A6.4** Migration guide написан.
- **A6.5** Production deployment ready.

### 6.7 Plan 123.7 acceptance (A7.1-A7.6)

- **A7.1** Effect-surface inference produces field-write-set.
- **A7.2** Caches survive calls where callee `∉` write-set.
- **A7.3** Conservative fallback when annotation missing.
- **A7.4** Regression — 0 new FAIL.
- **A7.5** Spec D-block для IPA.
- **A7.6** Cross-module fixtures ≥10 PASS.

### 6.8 Umbrella-level acceptance (Plan 123 total)

- **AU.1** Все sub-plans 123.1-123.7 ✅ closed.
- **AU.2** Comparative analysis vs Go/Rust/TS/Kotlin/Java
  match-or-exceeds (9 из 11 capabilities + 2 Nova-only).
- **AU.3** Full regression: 0 new FAIL across ~1500+ existing
  fixtures (baseline vs V7).
- **AU.4** Hot-path benchmarks (ReadBuffer/StringBuilder/HashMap)
  показывают ≥30% overall improvement в `-O0` build.
- **AU.5** Production deployment artifacts landed (telemetry,
  rollback guide, edition gating ready).

---

## 7. Risk register

### 7.1 Per sub-plan risks

**Plan 123.1:**
- **R-1.1: closure capture aliasing.** Closure body modifies cached
  field → stale cache. Mitigation: V1 conservative skip caching когда
  any closure references field.
- **R-1.2: vtable dispatch type.** Cache type должен match concrete
  receiver type. Mitigation: pass invokes ПОСЛЕ type-check.
- **R-1.3: AST mutation debugging UX.** Mitigation: debug-info maps
  generated locals back to source.

**Plan 123.2 (LICM):**
- **R-2.1: loop-carried mutation invisible.** Method call внутри
  loop body может indirectly mutate `@field`. Mitigation: skip LICM
  when ANY call in loop body (V1 conservative; V2 with IPA — refine).
- **R-2.2: early-return interaction.** Hoisted load executed even
  если loop never runs. Mitigation: only hoist read-only fields
  (loads side-effect-free).

**Plan 123.3 (#pure caching):**
- **R-3.1: `#pure` annotation drift.** User annotates method `#pure`
  but it's not (regression). Mitigation: type-checker enforces
  `#pure` constraints (D120 existing infrastructure).
- **R-3.2: pure-method allocation.** `#pure` method may allocate
  (e.g. `.collect()`) — caching reduces allocations BUT changes
  observable allocation count. Mitigation: distinguish
  «observably pure» (no allocs visible) from «contract pure».

**Plan 123.4 (chain):**
- **R-4.1: stack-frame bloat.** Many cached intermediates →
  stack frame grows. Mitigation: `--field-cache-max=N` global
  cap (default 8 cache locals per fn).

**Plan 123.5 (LSP):**
- **R-5.1: LSP perf regression.** Code-lens computation cost.
  Mitigation: cache metadata в incremental rope (Plan 104.1
  infrastructure).

**Plan 123.6 (telemetry):**
- **R-6.1: telemetry overhead.** Mitigation: opt-in flag, off by
  default in production.

**Plan 123.7 (IPA):**
- **R-7.1: incremental compilation invalidation.** IPA cross-module
  → change in callee invalidates caller cache. Mitigation: precise
  dependency tracking (future Plan 03.x infrastructure).

### 7.2 Cross-cutting risks

- **R-X.1: spec creep.** Multiple D-blocks (D217-D219+) — risk of
  inconsistency. Mitigation: D217 = umbrella semantics; D218/D219
  cross-ref D217.
- **R-X.2: compile-time budget overrun.** Cumulative passes (V1-V7)
  may exceed 5% overhead. Mitigation: per-sub-plan budget (1%
  each); telemetry verifies.
- **R-X.3: edition compatibility lock-in.** If user code relies on
  caching behavior (e.g. for benchmarking), edition migration tricky.
  Mitigation: semantic equivalence guarantee — by definition
  user code shouldn't be able to detect caching except via timing.

---

## 8. Production-grade requirements

### 8.1 Semantic equivalence guarantee

**Formal property:** for every input AST `A`, transformed AST
`T(A)`, и any input data — observable behavior of running `T(A)`
identical to running `A`. «Observable» = output / panic / exit
code / file-system effects / network effects.

**Verification methods:**
1. **Unit tests** на AST level: hand-crafted input → expected
   output AST.
2. **Codegen-level diff:** `.c` before/after — verify cache
   insertion points + read replacements.
3. **Semantic test:** `.nv` programs run identically с/без
   Plan 123 (compare `nova test` outputs byte-by-byte).
4. **Property-based tests:** random AST generator + equivalence
   check — generate 1000+ random programs, verify identical
   behavior.
5. **Differential testing:** existing nova test suite (~1500
   fixtures) — baseline OFF vs ON comparison.

### 8.2 Cross-platform determinism

Same input AST → same output AST → same `.c` (modulo platform-
specific runtime references). Verified through:
- CI matrix Windows MSVC / Linux clang / macOS clang.
- Bit-identical `.c` output check (где applicable).

### 8.3 Bounded overhead

| Resource | Budget | Verification |
|---|---|---|
| Compile-time per fn | <5% | `nova bench` compile suite |
| Stack-frame growth | ≤8 cache locals per fn | static check |
| Binary size | ≤2% bloat | `cmake --build` size diff |
| Debug-info size | ≤5% growth | DWARF section size diff |

### 8.4 Escape hatch

`--field-cache-threshold=0` — feature OFF (для diagnosis).
`--field-cache-threshold=N` — set custom threshold (default 2).
`--field-cache-max=N` — cap caches per fn (default 8).
`--no-licm` — disable Plan 123.2 LICM (default ON если 123.2
landed).
`--no-pure-cache` — disable Plan 123.3 pure caching.
`--no-chain-cache` — disable Plan 123.4 chain.

### 8.5 Edition compatibility

Feature ties к edition (Plan 62.F.bis). Old-edition code не
получает оптимизацию — safe fallback если выявится regression.
V1 (Plan 123.1) — enable unconditionally (semantic equivalence
guarantee достаточная); future versions могут require edition opt-in.

### 8.6 Debug-info preservation

Generated locals мапятся к source positions через DWARF
(Linux/macOS) или PDB (Windows). Debugger показывает
`_at_<field>` локалы с reference обратно к source `@<field>`
expression. Step-through debugging works через transformed code
(span preserved через AST node cloning).

### 8.7 Telemetry (production)

Metrics emitted (через Plan 57 bench infrastructure):
- `field_cache_methods_affected_pct` — % методов с инжекцией caches
- `field_cache_count_median` — median caches per affected method
- `field_cache_count_p99` — p99 caches per affected method
- `field_cache_compile_time_overhead_us` — per-fn pass time
- `field_cache_disabled_reason_counts` — reasons skipped (closure /
  perform / vtable / threshold-not-met)

---

## 9. Testing strategy (cross-cutting)

### 9.1 Test categories

**Per sub-plan tests** (в `nova_tests/plan123_N/`):
- Positive ≥N per sub-plan: cache emitted correctly.
- Negative ≥M per sub-plan: cache NOT emitted (conservative safety).
- Edge cases: corner patterns specific к sub-plan.

**Cross-cutting tests** (в `nova_tests/plan123/`):
- Semantic equivalence: pairs ON/OFF runs.
- Regression: full nova test 0 new FAIL.
- Property-based: random AST equivalence.

**Performance tests** (в `bench/plan123/`):
- Hot-path benchmarks: ReadBuffer/WriteBuffer/StringBuilder/HashMap.
- Compile-time benchmarks: large module pass time.
- Memory benchmarks: stack-frame size delta.

### 9.2 Verification: release nova-cli + clang

Все тесты через release nova-cli + clang (per
`feedback_nova_test_one_pass`):

```
cd nova-cli && cargo run --release --bin nova --quiet -- test \
  ../nova_tests/plan123_<N>/
```

### 9.3 Differential testing protocol

Для каждого sub-plan:
1. Baseline: full nova test без Plan 123 (commit hash X).
2. Plan 123.N ON: full nova test (commit hash Y).
3. Diff: byte-identical output где applicable; semantic
   equivalence остальное.
4. Acceptance: 0 new FAIL, 0 new regression.

---

## 10. Spec D-blocks decomposition

| D-block | Sub-plan | Scope | Status |
|---|---|---|---|
| **D217** | 123.1 | Method-local receiver field caching (core semantics, ro + mut + heuristics + safety) | NEW |
| **D218** | 123.2 | Loop-invariant code motion для receiver fields | NEW |
| **D219** | 123.3 | Pure-call result caching (effect-aware) | NEW |
| D217 amend | 123.4 | Chain caching extension | AMEND |
| D217 amend | 123.7 | Inter-procedural extension | AMEND |

Cross-references:
- D32 (semantics передачи параметров) — receiver semantics.
- D52 (объявление типов) — record/sum field declarations.
- D120 (#pure views + axioms) — pure annotation infrastructure.
- D131 (consume types) — linearity hints для aggressive caching.
- D175 (readonly field freeze) — ro field invariant.
- D176 (readonly T modifier) — same.

### 10.1 D217 outline

- **Section 1:** semantics — formal property (observable behavior
  preservation).
- **Section 2:** heuristics — threshold N, `ro` unconditional,
  `mut` straight-line.
- **Section 3:** safety constraints — closures / perform / vtable /
  generic mono / consume types.
- **Section 4:** mangling — `_at_<field>` naming convention,
  collision avoidance.
- **Section 5:** escape hatch — `--field-cache-threshold=0` and
  related flags.
- **Section 6:** debug-info — DWARF/PDB mapping.
- **Section 7:** edition compatibility — opt-in / opt-out.
- **Section 8:** cross-platform determinism.
- **Section 9:** cross-refs to D32/D52/D120/D131/D175/D176.

### 10.2 Q-resolution

Open questions проверить + закрыть через D217-D219:
- Q-codegen-cse-semantics (если существует) → D217.
- Q-debug-info-cache → D217 §6.
- Q-pure-call-cache → D219.
- Q-licm-correctness → D218.

---

## 11. Documentation deliverables

### 11.1 User-facing docs

- `docs/field-cache-optimization.md` (V5 → 123.5) — user guide:
  что делает feature, как читать LSP code-lens, escape hatch
  usage, performance expectations.
- `docs/migration/123-receiver-field-cache.md` (V6 → 123.6) —
  migration guide если выявится regression: edition pinning,
  threshold tuning, debugging.

### 11.2 Developer-facing docs

- `compiler-codegen/src/passes/field_cache/README.md` — code
  organization, extension points для future sub-plans.
- Code comments — каждая heuristic + safety constraint
  объяснена inline.
- D217-D219 spec sections — formal semantics.

### 11.3 `nova doc` integration

Если method получит cache через Plan 123 — `nova doc` page
показывает «N field caches optimized» badge (similar Plan 100.8
`[consume]` badge).

---

## 12. Tools / model settings

- **Opus 4.7 + Thinking ON** для всех sub-plans — compiler
  analysis design-heavy.
- Release nova-cli builds после каждой codegen edit.
- Tests via release nova-cli + clang (per
  `feedback_nova_test_one_pass`).
- Property-based testing infrastructure: hand-write generator
  (no external crate dep).
- Perf benchmarks через Plan 57 (`nova bench`) infrastructure
  (уже landed).

---

## 13. Closure status

🆕 PLANNED (umbrella). Каждый sub-plan flip'ает ✅ CLOSED
независимо. Финальный umbrella close после ALL 7 sub-plans ✅.

**Sub-plan status tracker:**

| Sub-plan | Status |
|---|---|
| 123.1 Core CSE | ✅ CLOSED 2026-06-02 (V1 active, D217 landed) |
| 123.2 LICM | ✅ CLOSED 2026-06-02 (V2 active, D218 landed) |
| 123.3 Pure caching | ✅ CLOSED 2026-06-02 (V3 active, D219 landed) |
| 123.4 Chain | ✅ CLOSED 2026-06-02 (V4 active, D217 amend landed) |
| 123.5 LSP/diag | 🆕 PLANNED |
| 123.6 Telemetry | 🆕 PLANNED |
| 123.7 IPA | 🆕 PLANNED |
| **Umbrella** | 🆕 PLANNED |

---

## 14. Next steps (immediate)

1. **Spawn Plan 123.1 sub-plan doc** (`docs/plans/123.1-core-cse.md`)
   с detailed Ф.0-Ф.6 implementation steps. Это immediate next
   action когда start реализация.
2. **Investigation Ф.0:** AST visitor infrastructure review +
   DECISION-A/B/C/D finalization.
3. **Commit + push** scaffold + investigation doc.
4. **Implementation Ф.1-Ф.6** через established Plan 59.1 pattern
   (фаза → commit → next).

---

## 15. Execution protocol (autonomous agent)

Для self-contained autonomous execution (минимальный prompt). Plan 59.1
closure (commits `7d9f4ba2911` → `9c37cde96c9`) — reference pattern.

### 15.1 Worktree setup (already done для umbrella)

- Worktree: `d:/Sources/nv-lang/nova-p123` ✅ создан.
- Ветка: `plan-123-receiver-field-cse` ✅ pushed.
- libuv submodule: ✅ scaffolded.
- Env vars для tests (per `project-worktree-nova-test-setup`):
  ```
  NOVA_GC_LIB_DIR="d:/Sources/nv-lang/nova/compiler-codegen/vcpkg_installed/x64-windows-static/lib"
  NOVA_GC_INCLUDE_DIR="d:/Sources/nv-lang/nova/compiler-codegen/vcpkg_installed/x64-windows-static/include"
  ```

### 15.2 Sub-plan ordering (recommended)

Start с **Plan 123.1 (Core CSE V1 foundational)** — gate для всех
остальных. Если 123.1 closed успешно, дальше 123.2 / 123.3 (parallel-
able) ИЛИ stop здесь (V1 уже shippable).

### 15.3 Per sub-plan execution cycle

Для каждого Plan 123.N:

1. **Spawn sub-plan doc** `docs/plans/123.N-<slug>.md` с детальной
   Ф.0-Ф.6 разбивкой. Template — копировать структуру Plan 59.1 doc
   (`docs/plans/59.1-generic-anon-tuple-mono.md` в main repo, merged).
   Sections: Контекст / Design / Phases / Acceptance / Risk register /
   Production-grade / Tools / Closure status.
2. **Commit scaffold** один commit `docs(plan 123.N): scaffold — ...`.
3. **Ф.0 Investigation:** repro / probe / DECISION-A/B/C/D finalize.
   Artifact: `docs/plans/123.N-artifacts/investigation.md`. Commit
   `docs(plan 123.N Ф.0): investigation + repro`.
4. **Ф.1-Ф.3 Implementation:** код + commits per фаза с pattern
   `feat(plan 123.N Ф.X): <что>`. Если фаза small — можно
   batch'нуть с Ф.X+1, но default — один commit per Ф.
5. **Ф.4 Tests:** positive + negative + property fixtures в
   `nova_tests/plan123_N/`. Verify через release nova-cli + clang:
   ```
   cd d:/Sources/nv-lang/nova-p123/nova-cli && \
     NOVA_GC_LIB_DIR=... NOVA_GC_INCLUDE_DIR=... \
     cargo run --release --bin nova --quiet -- \
     test ../nova_tests/plan123_N/
   ```
   Commit `feat(plan 123.N Ф.4): tests — N positive + M negative`.
6. **Ф.5 Spec:** D-block NEW в `spec/decisions/02-types.md` (или
   `08-runtime.md` если codegen) + amend cross-refs + README spec
   entry. Commit `docs(plan 123.N Ф.5): D2XX NEW + cross-refs`.
7. **Ф.6 Closure:** обновить **3 лога**:
   - `docs/simplifications.md` — closure section (CLOSED markers +
     acceptance summary + design lessons).
   - `docs/project-creation.txt` — chronological entry (full
     rationale + closes/unblocks).
   - `d:/Sources/nv-lang/nova-private/discussion-log.md` — process
     log (motivation, DECISION reasoning, lessons).
   - Plan doc status flip `🆕 PLANNED → ✅ CLOSED <date>`.
   - Commit `docs(plan 123.N Ф.6): closure logs`.

### 15.4 Regression verification

Перед finalize каждого sub-plan:
- **plan90/plan90_1/plan91_7/plan59_1** suites — quick (~5 min).
- **Targeted hot-path** (whatever sub-plan touches).
- **Full nova test** — final gate перед push (можно background — long).

Compare с baseline на main `HEAD@{merge-base}`:
- 0 NEW FAIL обязательно.
- Existing flakies (armed-M:N в concurrency) — verify identical
  failure pattern на main (per `feedback-isolated-worktree`).

### 15.5 Push protocol

- Per-phase commits — стандарт.
- Push на github ПОСЛЕ Ф.6 closure (или раньше если хочется backup
  — но не обязательно):
  ```
  cd d:/Sources/nv-lang/nova-p123 && git push 2>&1 | tail -3
  ```
- **NO merge в main** без явного user instruction.

### 15.6 Final status (per umbrella close)

После закрытия всех 7 sub-plans (V7):
- Plan 123 umbrella status flip `🆕 PLANNED → ✅ V7 CLOSED <date>`.
- Sub-plan status tracker в §13 — все ✅.
- Umbrella-level closure entry в 3 логах.
- Final commit `docs(plan 123 umbrella): V7 closure — all 7 sub-plans CLOSED`.

### 15.7 Bail-out conditions

Остановиться + поднять user:
- Любая P0 регрессия (existing нерегрессионный test FAIL).
- Architectural blocker (e.g. AST visitor infrastructure не такая
  как ожидалось — design decision требует input).
- D-block conflict (D217 collision с другим plan'ом — coordinate).
- Cumulative time >2× estimate (что-то идёт не так).

### 15.8 Memory + feedback refs

Эти feedback memories обязательно соблюдать:
- `feedback-isolated-worktree` — worktree per план, не переключать
  ветки.
- `feedback_worktree_cwd_clarity` — cd-prefix в каждой Bash команде.
- `feedback-worktree-auto-register` — register first Bash command.
- `feedback_git_add_specific` — никогда `git add -A`.
- `feedback-verify-index-before-commit` — `git diff --cached --stat`
  перед commit.
- `feedback-no-claude-coauthor` — никогда не добавлять
  `Co-Authored-By: Claude`.
- `feedback-commit-per-task` — split на multiple commits.
- `feedback-update-logs` — 3 лога после каждой big task.
- `feedback-no-external-memory-for-project-state` — для plan/D status
  читать docs/plans/README.md, не external memory.
- `feedback_nova_syntax` — никогда не выдумывать syntax, проверять
  через parser probe.
- `feedback_nova_test_one_pass` — capture summary + FAIL details в
  одном запуске.
