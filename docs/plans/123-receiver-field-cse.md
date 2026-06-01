// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 123 — Method-local receiver field caching (transparent `@field` CSE)

> **Создан 2026-06-01.**
> **Status:** 🆕 PLANNED.
> **Приоритет:** P2 — codegen perf + readability win. Не блокер релиза,
> но повсеместная оптимизация (любой method с `@field` accessed N+ раз).
> Особенно ценно для hot-path кодеков типа ReadBuffer/WriteBuffer/
> StringBuilder/HashMap iter — где `@data[p]` patterns в tight loops.
> **Оценка:** ~2-3 dev-day.
>   - Investigation / DECISION-A/B/C/D: ~0.5 day
>   - Codegen pass implementation (ro fields): ~0.5 day
>   - Mut field analysis (write-region detection): ~0.5 day
>   - Edge cases (branches/loops/closures/`@field.method()`): ~0.5 day
>   - Tests (positive + negative + perf bench): ~0.5 day
>   - Spec + logs + closure: ~0.5 day
> **Зависимости:** —. Pure codegen optimization, не зависит от других
>   pending плана. Plan 02 (codegen-c-backend, активный) — родственная
>   тема (broad codegen umbrella), но Plan 123 — самостоятельная
>   оптимизация которую можно ship'нуть независимо.
> **D-блоки:** **новый D217** — Method-local receiver field caching
>   (semantics + heuristics + safety constraints).
> **Worktree convention:** `nova-p123` (создан через `git worktree add
>   d:/Sources/nv-lang/nova-p123 -b plan-123-receiver-field-cse
>   github/main`; libuv submodule scaffolded per
>   `project-worktree-nova-test-setup`).
> **Recommended model:** **Opus 4.7 + Thinking ON** — codegen analysis
>   pass design-heavy (write-region detection для mut fields, branch
>   safety, AST visitor recursion).

---

## 1. Контекст

### 1.1 Проблема

Пользователь пишет идиоматичный Nova-код:

```nova
export fn ReadBuffer mut @try_read_u32_le() -> Result[u32, ReadBufferError] {
    if @data.len() - @pos < 4 {
        return Err(UnexpectedEnd { wanted: 4, available: @data.len() - @pos })
    }
    ro p = @pos
    ro b0 = @data[p] as int
    ro b1 = @data[p + 1] as int
    ro b2 = @data[p + 2] as int
    ro b3 = @data[p + 3] as int
    @pos = p + 4
    Ok((b0 | (b1 << 8) | (b2 << 16) | (b3 << 24)) as u32)
}
```

Codegen эмитит **literal `self->X`** на каждый `@X`-доступ:
- `@data.len()` × 2 → `self->data->len` × 2
- `@pos` × 2 → `self->pos` × 2
- `@data[p+i]` × 4 → `self->data->data[i]` × 4

Итого **success-path ~7-8 dereferences** через `self->`, хотя
семантически читаются 2 stable значения (`data` ptr + `pos` int).

C-компилятор под `-O2` теоретически делает CSE, но:
1. Conservative aliasing analysis для `mut self->pos` блокирует
   подъём `self->data` (потенциально другой write мог изменить).
2. Под `-O0`/`-O1` (dev / debug builds) CSE отсутствует — каждый
   `self->X` literal load.
3. Inlining ограничения (большие методы, fn-pointer dispatch через
   vtable) дополнительно ограничивают C-уровень CSE.

### 1.2 Решение (concept)

Transparent compile-time оптимизация в codegen: для каждого
`@field` accessed N+ раз в method body **single hoisted local cache**
эмитится в начале функции; subsequent reads через cached local.

```c
// Before:
static Result self_method(Nova_ReadBuffer* self) {
    if (self->data->len - self->pos < 4) { ... }
    nova_int p = self->pos;
    nova_int b0 = self->data->data[p];        // self->data deref again
    nova_int b1 = self->data->data[p + 1];    // ...
    nova_int b2 = self->data->data[p + 2];
    nova_int b3 = self->data->data[p + 3];
    self->pos = p + 4;
    return Ok((b0 | ...));
}

// After (Plan 123):
static Result self_method(Nova_ReadBuffer* self) {
    NovaArray_nova_byte* _at_data = self->data;  // hoisted cache (ro)
    nova_int _at_pos = self->pos;                // hoisted cache (mut)
    if (_at_data->len - _at_pos < 4) { ... }
    nova_int p = _at_pos;
    nova_int b0 = _at_data->data[p];
    nova_int b1 = _at_data->data[p + 1];
    nova_int b2 = _at_data->data[p + 2];
    nova_int b3 = _at_data->data[p + 3];
    self->pos = p + 4;                            // write — direct, не cache
    return Ok((b0 | ...));
}
```

Result: dev-build / hot-path методы получают «бесплатный» CSE без
зависимости от C-optimizer'а. Production-built code (`-O2+`) тоже
выигрывает в edge cases где clang aliasing analysis conservative.

### 1.3 Прецеденты в других языках

- **Rust:** в RAW `&self.field` MIR-passes делают это автоматически
  (escape + alias analysis); user pisheт naturally.
- **Swift:** ARC + escape analysis позволяет CSE owned reads.
- **Kotlin:** JVM HotSpot escape analysis + scalar replacement.
- **Go:** компилятор делает CSE на SSA-уровне; conservative для
  shared-state, но `m.field` reads commonly hoisted.

Nova — bootstrap codegen (без SSA/MIR layer'а) — делает literal
`self->field` каждый раз. Plan 123 добавляет **explicit AST/codegen-
уровень** оптимизацию без необходимости вводить полный SSA.

---

## 2. Design

### 2.1 Where: AST-rewrite vs codegen-time?

**Альтернатива A: AST rewrite pass.**
- Pre-codegen pass walks FnDecl.body
- Counts `@field` reads + write sites
- Если threshold reached → insert `ro _at_<field> = @<field>` at body start
- Replace all subsequent `@<field>` (reads) → `_at_<field>` Ident
- Writes (`@<field> = X`) — оставить как есть OR re-cache после
- Pros: clean separation, downstream codegen unchanged
- Cons: AST mutation, debugging step shows transformed code, span
  preservation work

**Альтернатива B: codegen-time hoisting.**
- Codegen ведёт `field_caches: HashMap<String, String>` per fn scope
- При первом emit `self->X` → emit local `_at_X = self->X;` + return
  cache var name
- Subsequent → return cached var
- Writes → invalidate cache: emit `self->X = ...`, then `_at_X = self->X;`
- Pros: no AST mutation, span preserved
- Cons: codegen logic complicates, harder to unit test

**DECISION-A:** Альтернатива A (AST rewrite). Reasoning:
- AST passes уже существуют (consume-check, defer-rewrite, etc.) —
  паттерн знакомый
- Span preserved через AST node-level cloning
- Unit-testable как pure transformation
- Downstream codegen unchanged → no risk регрессии в emit_c.rs (где
  основная сложность)
- Hot-path emit_c.rs codegen остаётся simple

### 2.2 What/when to cache: heuristics

**`ro` fields (immutable):**
- ALWAYS cache if accessed **≥2 раз** в body
- Zero semantic risk — no writes possible
- Trivial heuristic, no analysis required

**`mut` fields (mutable):**
- Cache if accessed **≥2 раз** AND **straight-line region** между
  read и read (no write of same field в between)
- Conservative: если есть branch/loop/call между reads — split into
  multiple caches (each region independent) OR skip caching
- V1: cache только в "linear prefix" (read up to first write/branch)
- V2: full def-use analysis

**`@field.method(...)` patterns:**
- Cache `@field` part, not the method call result
- Method call has side effects (potentially), shouldn't be cached
- E.g. `@data.len()` × N → cache `@data` → emit `_at_data->len` × N
  (relies on C compiler to CSE the field load — usually trivial для
  array headers)

**`@nested.field.subfield`:**
- V1: только top-level `@field` cache
- V2: nested chain caching (`_at_data_inner = _at_data->inner;`)
  если sub-path accessed N+ раз

### 2.3 DECISION-B: threshold N

Default `N = 2` — cache если field accessed 2+ раз.
Rationale:
- N=1 — no benefit (single load anyway)
- N=2 — break-even / small win (1 extra local declaration vs 1 fewer
  pointer deref)
- N=3+ — clear win

Tunable через codegen flag `--field-cache-threshold=N` (default 2).
Если flag = 0 — feature disabled (escape hatch для debugging).

### 2.4 DECISION-C: naming convention

Generated local name: **`_at_<field>`** (mirrors `@<field>` Nova
syntax — "at-field" cache).

Collision avoidance: если user уже использует `_at_X` name (rare),
codegen renames `_at_X_<counter>`. Verify via parser scan.

### 2.5 DECISION-D: scope (V1 vs V2)

**V1 (этот план):**
- Bare `@field` read patterns (most common)
- `ro` fields — unconditional cache при ≥2 reads
- `mut` fields — straight-line prefix only
- `@field.method()` — cache `@field`, не method result
- Top-level `@field` only (no nested chain caching)
- Method-local scope (per-fn analysis, no cross-call)

**V2 (deferred, потенциально Plan 123.1):**
- Full def-use analysis для mut fields
- Cache invalidation в loops / nested branches
- Nested chain caching `@a.b.c`
- Cross-call analysis (если other method'ы guaranteed pure)

### 2.6 Safety constraints (production-grade)

1. **Closures capturing `@field`:** если closure body references
   `@field`, и closure created в method body — closure capture
   resolution должна работать с either original или cached local.
   Conservative: skip caching если any closure references same field.
2. **`@`-receiver mut alias:** если method body передаёт `@self`
   куда-то (e.g. `other.process(self)`), receiver mutation возможна
   из external call → invalidate ALL caches before/after такого call.
3. **Effect handlers:** если method body содержит `perform Effect.X`
   call — handler может re-enter method (recursive call через
   handler stack). Conservative: cache invalidate перед каждым
   `perform`.
4. **Generic methods:** mono'd instance каждый получает свою cache
   (ortho к Plan 59.1 mono'd schema).
5. **Trait/protocol methods через vtable:** receiver type unknown
   at codegen time → cache применяется к receiver-typed local
   (mono'd через D122 hybrid dispatch).

---

## 3. Phases

### Ф.0 — Investigation + DECISION confirmation (GATE)

**Что:** sanity-check существующего AST visitor infrastructure (где
сидит consume-check / defer-rewrite), build mental model. Finalize
DECISION-A/B/C/D через analysis of edge cases в std/runtime/ —
выявить any pattern который мог бы surprise.

**Output:**
- DECISION-A/B/C/D finalized (или revised based on findings).
- Investigation doc в `docs/plans/123-artifacts/investigation.md`.
- List representative call sites из std/ + nova_tests/ — методы
  с ≥3 `@field` reads (для benchmark target).

**Exit criteria:**
- ✅ AST visitor infrastructure identified.
- ✅ List of 10+ benchmark target methods (e.g. ReadBuffer.*,
  WriteBuffer.*, StringBuilder.*, HashMap.*).
- ✅ Edge case audit complete (closures / perform / vtable / mut alias).
- ✅ Plan Ф.1-Ф.5 scope confirmed.

### Ф.1 — AST rewrite pass for `ro` fields (V1 core)

**Что:** реализовать AST pass который walks `FnDecl.body`, counts
`@field` reads per `ro` field, и emit'ит `ro _at_<field> = @<field>`
prefix declaration + rewrites all subsequent `@<field>` reads.

**Implementation:**
1. New file `compiler-codegen/src/passes/field_cse.rs` (или extend
   existing pass module).
2. `fn rewrite_field_cse(fn_decl: &mut FnDecl) -> ()`.
3. Walk body: collect `MemberAccess { obj: SelfExpr, name }` reads
   per field name (`HashMap<String, Vec<NodeRef>>`).
4. For each `ro` field с count ≥ threshold:
   - Insert at body start: `Stmt::Let(Pattern::Ident(_at_<f>), ...,
     value=MemberAccess{SelfExpr, f})`.
   - Replace each `@field` read node with `Ident(_at_<f>)`.
5. Receiver-type lookup для distinguish `ro` vs `mut` fields:
   из existing TypeRegistry / record_schemas.

**Hook:** invoke pass в pipeline ПОСЛЕ type-check (нужны типы для
field classification) НО ПЕРЕД codegen.

**Exit criteria:**
- ✅ Pass implemented + unit tests (~5 cases) PASS.
- ✅ Re-compile std/runtime/read_buffer.nv → emit'ёт cached locals.
- ✅ plan91_12 buffer tests PASS (semantic equivalence).
- ✅ Generated .c показывает `_at_data = self->data;` prefix в
  affected methods.

### Ф.2 — Mut field caching with straight-line region analysis

**Что:** extend pass для `mut` fields с conservative analysis.

**Algorithm (V1 conservative):**
1. Walk body **linearly** до first occurrence of:
   - Direct write `@field = X` (для этого `field`)
   - Branch (`if`/`match`/`while`/`for`)
   - `perform` effect call
   - External call (`other.process(self)` etc.)
2. Count `@field` reads до этого cutoff point.
3. Если count ≥ threshold → cache via local; rewrites limited to
   reads before cutoff.
4. После cutoff — reads остаются `@field` (uncached).

**Exit criteria:**
- ✅ ReadBuffer.try_read_u32_le методы → both `@data` (ro) and
  `@pos` (mut) cached корректно.
- ✅ Branch / write-after-read cases handled conservatively (no
  semantic mismatch).
- ✅ Test fixtures plan123/ — cover straight-line / branch /
  write-then-read / loop cases.

### Ф.3 — Edge cases: branches, loops, closures, `perform`

**Что:** robust handling выявленных edge cases.

1. **Branch:** cache visible только within branch arms где no write
   к field. Multiple branches → cache emit'ится в каждой arm
   (если worth it per local threshold).
2. **Loop:** cache valid within loop body iff no write inside body.
   Иначе skip caching для этого field.
3. **Closure:** если closure captures `@field`, V1 skips caching для
   this field (conservative).
4. **`perform Effect.X`:** invalidate ALL active caches перед
   `perform` (handler может re-enter); re-cache после если field
   accessed снова.

**Exit criteria:**
- ✅ Edge case test fixtures (1 test per pattern, 4 total) PASS.
- ✅ No false-positive caching (semantic equivalence preserved).

### Ф.4 — Tests + perf benchmark

**Positive (≥10):**
- Basic ro field cache: ReadBuffer-style `@data` × N reads.
- Mut field cache straight-line: `@pos` cached perfix.
- Both ro + mut combined.
- Single-read field: NO cache (below threshold).
- `@field.method()` patterns.
- Multiple fields cached separately.
- Generic method instantiation: caches per mono.
- Constructor / static method: no `@`-receiver, no pass.
- Inline expression methods (`=>`): no body → skip cleanly.
- Method без `@field` references вообще: no-op.

**Negative (≥5):**
- Cache invalidation after `@field = X` write: subsequent read uses
  fresh `self->field`, not stale `_at_field`.
- Closure capture: field NOT cached (conservative).
- `perform` invalidation: cache re-fetched after.
- External call with self-alias: re-fetch.
- `--field-cache-threshold=0` (escape hatch): no cache emitted.

**Perf benchmark:**
- 1 fixture в `bench/plan123/` — measure ReadBuffer.try_read_u32_le
  loop 1M iterations. Compare `-O0` baseline (Plan 123 OFF) vs
  Plan 123 ON. Expected: 10-30% improvement в dev-build hot loops.

**Verification:** все тесты через release nova-cli + clang
(`cargo run --release --bin nova -- test ...`). Bench через
`nova bench`.

**Exit criteria:**
- ✅ plan123 fixtures ≥15 (10+ positive, 5+ negative) ALL PASS.
- ✅ Regression: full nova test 0 new FAIL (compare baseline на
  main commit перед Plan 123).
- ✅ Perf benchmark показывает positive delta для hot loops.

### Ф.5 — Spec D-block + Q reconciliation

**Что:**
1. **D217 NEW** (`spec/decisions/02-types.md` или `08-runtime.md` —
   в зависимости от классификации):
   - Полное описание Method-local receiver field caching
   - Semantics (equivalence guarantee)
   - Heuristics (threshold, ro vs mut)
   - Safety constraints (closures / perform / vtable)
   - Escape hatch (`--field-cache-threshold=0`)
   - Cross-refs (D32 / D52 / D131 / D175 / D176 — field semantics).
2. **D D-block index** (`spec/decisions/README.md`): D217 entry.
3. **Q-resolution:** проверить если есть Q-codegen-cse или
   Q-field-loads-hoisting в open-questions.md; закрыть через D217.

**Exit criteria:**
- ✅ D217 написан, cross-references valid.
- ✅ README index D217 row added.
- ✅ Open Q (если есть релевантные) closed.

### Ф.6 — Acceptance + closure logs

**Что:**
1. Verify все A1-A_N criteria (см. §4).
2. Update `docs/simplifications.md` — Plan 123 closure entry
   (CLOSED markers, acceptance summary, design lessons).
3. Update `docs/project-creation.txt` — chronological prose log
   с full design rationale + benchmarks results.
4. Update `nova-private/discussion-log.md` — process log: motivation
   (ReadBuffer optimization discussion), DECISION-A/B/C/D reasoning,
   findings, lessons.
5. Update Plan 123 doc — Status flip `🆕 PLANNED → ✅ CLOSED <date>`.
6. Final commit closure.
7. Push branch `plan-123-receiver-field-cse` → github (no merge —
   user explicit instruction).

**Exit criteria:**
- ✅ Все A1-A10 PASS.
- ✅ Три лога обновлены, plan doc status flipped.
- ✅ Branch pushed.

---

## 4. Acceptance criteria

- **A1** `ro` field accessed 2+ раз → emitted `_at_<field>` local
  prefix, subsequent reads через cache.
- **A2** `mut` field accessed 2+ раз в straight-line region → emitted
  cache, write invalidates / re-caches.
- **A3** Branches / loops / `perform` properly invalidate cache
  (semantic equivalence preserved).
- **A4** Closure capturing `@field` → skip cache (conservative).
- **A5** Escape hatch `--field-cache-threshold=0` отключает feature.
- **A6** ReadBuffer.try_read_u32_le before/after .c diff показывает
  reduction `self->data->...` from 5 to 1 + cache prefix.
- **A7** Regression — full nova test 0 new FAIL (baseline vs Plan 123 ON).
- **A8** Spec D217 NEW + README spec entry — landed.
- **A9** plan123 fixtures ≥15 (10+ positive, 5+ negative) ALL PASS.
- **A10** Perf benchmark показывает ≥10% improvement для
  ReadBuffer hot loop (`-O0` dev build).

---

## 5. Risk register

- **R-1: closure capture aliasing.** Если closure body modifies
  captured `@field` (через bound `@self` reference), cache не
  invalidated. Mitigation: V1 conservative skip caching когда any
  closure references field (даже read-only).
- **R-2: vtable dispatch.** Mono'd methods через vtable могут получить
  receiver через `void* self` cast'нутый к concrete type. Cache type
  должен match concrete type. Mitigation: pass invoke'tcs ПОСЛЕ
  type-check (известны concrete types).
- **R-3: AST mutation breaks debugging.** Step debugger показывает
  `_at_field` вместо `@field` в transformed code. Mitigation: span
  preserved через node cloning; debug-info maps generated locals
  обратно к source `@field` reads.
- **R-4: regression в edge case.** Existing test PASS на baseline,
  FAIL на Plan 123 codegen pass. Mitigation: full regression suite
  обязателен перед closure; `--field-cache-threshold=0` escape hatch
  для diagnosis.
- **R-5: feature gates / conditional compilation.** `#if platform(...)`
  branches могут содержать different `@field` patterns; cache scope
  должен respect `#if` boundaries. Mitigation: AST pass runs AFTER
  conditional resolution.

---

## 6. Production-grade requirements

Per user instructions (2026-06-01): «реализовать без упрощений (как
для прода)».

- ✅ Никаких safety hatches типа `unimplemented!`/`panic!` в новом
  pass code — все edge cases handled или конкретно skipped per
  conservative rules.
- ✅ AST pass deterministic — same input → same output (no HashMap
  iteration order leak'и).
- ✅ Semantic equivalence — exhaustive test corpus с cases-of-
  concern (closures / perform / branches / loops).
- ✅ Escape hatch `--field-cache-threshold=0` — production users
  могут отключить если найдут regression.
- ✅ Diagnostic errors / warnings (если есть) — Plan 50 D102 format.
- ✅ Performance — pass должен add <5% к compile time per fn
  (heuristic check).
- ✅ Documentation — D217 полный, code comments в pass file
  объясняют each heuristic + safety constraint.

---

## 7. Tools / model settings

- **Opus 4.7 + Thinking ON** — codegen analysis pass design-heavy
  (write-region detection для mut fields, branch safety, AST
  visitor recursion edge cases).
- Release nova-cli builds после каждой codegen edit (no cargo dev
  cycle для testing fixtures).
- Tests via `cargo run --release --bin nova -- test ...` (per
  feedback_nova_test_one_pass).

---

## 8. Closure status

🆕 PLANNED — обновляется после каждой фазы. Финальный flip на
✅ CLOSED при завершении Ф.6.
