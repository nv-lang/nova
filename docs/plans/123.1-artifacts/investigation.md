// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 123.1 Ф.0 — Investigation + DECISIONs A-F

> **Дата:** 2026-06-01.
> **Worktree:** `d:/Sources/nv-lang/nova-p123`.
> **Branch:** `plan-123-receiver-field-cse`.

---

## 1. AST representation для `@field` доступа

Парсер десугарит `@field` (D17/D52) в один из двух форм AST:

```rust
// `@field`-shorthand (bare): представлено как Member { obj: SelfAccess, name }.
// Источник: compiler-codegen/src/ast/mod.rs:1421 — SelfAccess variant.
ExprKind::SelfAccess                              // bare `@` (rare)
ExprKind::Member { obj: SelfAccess-expr, name }   // `@field` (common)
```

В практике V1 целевой паттерн — `Member { obj.kind == SelfAccess,
name: "F" }`. Это легко detect'ить shallow visitor'ом.

**Mutation:** `@field = value` парсится как
`Stmt::Assign { target: Expr<Member{SelfAccess, F}>, op: AssignOp::Assign, value, span }`.
Compound (`@field += v`) — same target shape, разные `op`.

---

## 2. RecordField classification (`ro` vs `mut`)

`compiler-codegen/src/ast/mod.rs:804` — `RecordField`:
```rust
pub struct RecordField {
    pub name: String,
    pub ty: TypeRef,
    pub readonly: bool,    // ← D175 `ro` modifier
    pub mutable: bool,     // ← `mut` modifier (D108)
    pub is_embed: bool,
    pub embed_anonymous: bool,
    pub span: Span,
    pub consume: bool,     // ← D131 consume linearity
}
```

Map fields → kind:
- `readonly == true` → **Ro** (unconditional cache, V1 path).
- `mutable == true` → **Mut** (straight-line write-region cache).
- `consume == true` → **Skip** (D131 linearity, separate semantics).
- `is_embed == true` → **Skip** (use-style embed, separate dispatch).
- Default (neither flag) → **Mut** в bootstrap edition (treated як
  `mutable` для safety). Refinement при edition gating — Plan 123.5+.

---

## 3. Receiver type → fields mapping

Method body referenced `@F` имеет receiver type known:
- `FnDecl.receiver: Option<Receiver>` (`mod.rs:344`).
- `Receiver.type_name: String` (`mod.rs:554`).
- Module-level `Item::Type(TypeDecl)` with matching name содержит
  `TypeDeclKind::Record(Vec<RecordField>)` (`mod.rs:734`).

Алгоритм: pre-pass собирает `HashMap<String, HashMap<String,
FieldKind>>` (TypeName → FieldName → kind). При walk'е каждого
`FnDecl` с `receiver = Some(r)` — lookup `r.type_name` → field map.

**Edge cases:**
- Generic type: `type Box[T] { value T }` — `Receiver.type_name =
  "Box"`. Lookup by base name works.
- Sum type variant: `TypeDeclKind::Sum(...)` — методы-на-всю-сумму
  Receiver указывает на sum name, не на variant; variant fields
  доступны через pattern-match, не через `@field`. V1 — sum types
  обрабатываем как «no record-fields direct» → skip.
- Effect / Protocol types — `@field` не существует (нет state) → skip.
- Opaque types (D126) — `external type X` — no fields known → skip.
- `NamedTuple` (D215) — fields known через `NamedTupleField`. V1 —
  treat как ro (named tuples are stack values, immutable post-construct).

---

## 4. AST visitor templates

**Templates:**
- `compiler-codegen/src/desugar.rs` — pure trasformation,
  `desugar_module` → `desugar_item` → `desugar_stmt` → `desugar_expr`
  → `desugar_children` рекурсивно. Pattern: collect children mutably,
  replace node based on condition.
- `compiler-codegen/src/callnorm.rs` — same pattern + pre-pass
  collecting `Sigs` (signature map), затем walk с mutable ref на
  collected info.

`field_cache::cache_module` mirror'ит callnorm:
1. Pre-pass: build `FieldRegistry` (TypeName → FieldName → FieldKind).
2. Per-fn walk: read-count analysis + closure/perform detection.
3. Rewrite: insert prefix bindings, replace reads.

---

## 5. Pipeline integration points

Three commands invoke C-codegen pipeline:
- `compiler-codegen/src/main.rs::cmd_compile` (line 322).
- `compiler-codegen/src/main.rs::cmd_run` (line ~280, через
  Interpreter).
- `compiler-codegen/src/main.rs::cmd_test` (line 388, interpreter).
- `compiler-codegen/src/test_runner.rs::compile_to_c_inner` (line
  ~2200, `nova test` C-codegen path — primary target).

Pipeline order (per `compile_to_c_inner`):
```
parse → check_module_path → check_module → lints → verify
→ const_fn_eval::rewrite_const_fn_calls
→ types::annotate_map_literals
→ desugar::desugar_module
→ types::infer_effects
→ callnorm::normalize_module
→ [<-- INSERT field_cache::cache_module HERE -->]
→ codegen::CEmitter::emit_module
```

`cmd_run` / `cmd_test` (interpreter path) — insert после `desugar`
перед `Interpreter::load_module`. Interpreter тоже walks AST, видит
cached bindings прозрачно.

---

## 6. Closure detection

Closure types in AST (`mod.rs:1564-1598`):
- `ExprKind::Lambda` (deprecated, backward-compat).
- `ExprKind::ClosureLight` (Plan 19 — `|x| body`).
- `ExprKind::ClosureFull` (Plan 19 — `fn(...) body`).
- `ExprKind::HandlerLit` (effect handler — body содержит method bodies).
- `ExprKind::ProtocolLit` (protocol impl literal — same).

V1 strategy: pre-scan method body для closure occurrences. Если
ЛЮБОЙ closure body содержит `Member { SelfAccess, name: F }` — mark
field `F` as «captured by closure» → skip caching this field
in the entire fn body.

Rationale: closure capture aliasing — closure может outlive scope (e.g.
spawned fiber, stored handler) и mutate captured `self`-pointer'ом
field, что invalidates outer cache. V1 conservative — skip полностью.

---

## 7. Call-site (perform / method) invalidation

Pass detects call boundaries:
- `ExprKind::Call { func, args, trailing }` — общий вызов. После
  вызова mut-field cache invalidated (callee может mutate `@field`
  через alias / IPA не известен). Re-cache after если accessed снова.
- `ExprKind::Spawn`, `Supervised`, `Detach`, `Blocking` — concurrency
  primitives. Mut cache invalidated boundary.
- `ExprKind::With { bindings, body }` — effect handler — same.

Ro-fields — unaffected (frozen). Caching survives any boundary.

---

## 8. nova_tests fixture pattern

Format:
```nova
// Plan 123.1 Tx.y: description.

// EXPECT_COMPILE_ERROR <code>   ← для negative
// EXPECT_STDOUT <text>          ← для runtime check (optional)

module plan123_1.<name>

<test body>
```

Test runner (`nova test`) — invokes C-codegen pipeline +
`clang`-compiles output. Positive fixtures должны compile cleanly +
pass any embedded `test "name" { ... assert(...) }` blocks.
Negative — должны fail с matching error code.

V1 acceptance — positive fixtures verify semantic equivalence через
runtime asserts (cache emit'ится, но behavior identical).
Diff-against-baseline для `.c` output — separate verification step
(вручную для A1.6 ReadBuffer perf-demo).

---

## 9. DECISIONs A-F (finalized)

Все DECISIONs finalized в `docs/plans/123.1-core-cse.md` §2.1-§2.6:

- **DECISION-A:** pass position — после callnorm, перед codegen
  (C-path); после desugar, перед Interpreter::load_module (interp-path).
- **DECISION-B:** cache local naming = `_at_<field>` + numeric suffix
  при collision.
- **DECISION-C:** ro vs mut classification — `RecordField.readonly`
  unconditional, `mutable` straight-line, defaults treated как mut в
  bootstrap edition, consume / embed skipped.
- **DECISION-D:** default threshold N=2, `--field-cache-threshold=N` CLI.
- **DECISION-E:** V1 scope = direct `Member{SelfAccess, name}` only;
  chains / pure-call / LICM / IPA — separate sub-plans.
- **DECISION-F:** closure-skip / call-boundary invalidate (mut) /
  protocol-receiver skip / generic mono OK.

---

## 10. Risk-mitigation immediate

- **R-1.1 closure capture aliasing:** handled через closure pre-scan
  → conservative skip.
- **R-1.2 vtable dispatch type:** handled через protocol receiver skip.
- **R-1.3 debug-info UX:** handled через span preservation на cache
  let bindings.
- **R-1.4 name collision:** detect-before-emit + numeric suffix.
- **R-1.5 hidden mutation via call:** ANY call invalidates mut cache в V1.

---

## 11. Implementation outline (Ф.1-Ф.3 mapping)

**Ф.1 — `compiler-codegen/src/field_cache.rs`:**
- `pub struct FieldCacheConfig { threshold, max_per_fn, enabled }`.
- `pub fn cache_module(&mut Module, &FieldCacheConfig)`.
- `FieldRegistry` build.
- Per-fn walk + ro-field caching.
- Pipeline wiring в main.rs (cmd_compile / cmd_test / cmd_run) +
  test_runner.rs (compile_to_c_inner). For interpreter path
  (cmd_test / cmd_run) — wire через ENV var (`NOVA_FIELD_CACHE=off`)
  поскольку interpreter CLI legacy — flags только на test/build/compile.

**Ф.2 — extend `field_cache.rs`:**
- Write-region detection (Assign targeting SelfAccess Member).
- Region-bounded cache emit.
- Compound assign handling.

**Ф.3 — extend `field_cache.rs`:**
- Closure body scanner.
- Call-boundary invalidation для mut fields.
- Protocol receiver skip.

**Ф.4 — `nova_tests/plan123_1/`:** 10+ positive + 5+ negative + 3+
property fixtures.

**Ф.5 — `spec/decisions/08-runtime.md` D217 NEW:** ~200-300 lines per
umbrella §10.1 outline (Section 1-9).

**Ф.6 — 3 logs + status flip + push.**

---

## 12. CLI flag landing

Three flags added to `nova` binary (clap):
- `--field-cache-threshold <N>` (default 2, accepts 0..=255).
- `--field-cache-max <N>` (default 8).
- `--no-field-cache` (alias for `--field-cache-threshold=0`).

Wired через `Cmd::Compile`, `Cmd::TestBuild`, `Cmd::TestAll`.
Interpreter mode (`Cmd::Run` / `Cmd::TestInterp`) — same flags;
interpreter sees cached AST transparently.

---

## 13. Closure of Ф.0

DECISIONs A-F finalized. Implementation outline ready. Risk-mitigation
strategy defined. Pipeline integration points identified. Ready to
proceed Ф.1.
