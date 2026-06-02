// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 123.3 Ф.0 — Investigation + DECISIONs A.3-F.3

> **Дата:** 2026-06-02.

---

## 1. D24 Purity infrastructure

`FnDecl.purity: Purity` field (ast/mod.rs:402):
- `Purity::Pure` — assertion `@pure` annotation OR inferred via SCC.
- `Purity::Effectful` — has effects.
- `Purity::Unknown` — default (parser sets для не-annotated, type-
  checker resolves).

V3 caches calls only когда method's `purity == Purity::Pure`.
Conservative — Unknown НЕ cached (could be effectful).

## 2. AST shape for pure-call

`@<method>()` parses as:
```
Call {
    func: Member {
        obj: Box<Expr<SelfAccess>>,
        name: "<method>"
    },
    args: vec![],
    trailing: None
}
```

Detection: `expr_is_self_pure_call(e, fname, pure_methods, recv_type)`.

## 3. Pure method registry

Built single-pass in `cache_module`:
```
for each Item::Fn(f) in module.items {
    if let Some(recv) = &f.receiver {
        if f.purity == Purity::Pure && f.params.is_empty() {
            registry.insert((recv.type_name.clone(), f.name.clone()));
        }
    }
}
```

V3 limit: args-less only (`@len()` form). Args-with-literals — V3.1.

## 4. Composition with D218 and D217

Order in cache_module:
1. D218 LICM phase — hoist loop-invariant @F reads.
2. V3 pure-call phase — cache @<pure_method>() at body prefix.
3. D217 per-fn ro/mut caching — sees V3 locals as regular bindings.

Why order matters:
- D218 first: pure-call args may reference @F that LICM hoists; V3
  needs hoisted form.
- V3 before D217: cache pure-call result; D217 then caches @F
  reads OUTSIDE pure-call args.

## 5. Conservative invalidation

V3 simple rule: if `block_contains_write_to_any_field(body)` →
skip all pure-call caching. Rationale: method body не has @F = ...
→ pure method result invariant across body.

V3.1 followup: use D24 `f.reads` frame info — invalidate only when
written field intersects with what pure method reads.

## 6. DECISIONs A.3-F.3

- **A.3:** scope = `@<pure_method>()` args-less only.
- **B.3:** placement = body prefix. Naming `_at_<method>_call`.
- **C.3:** threshold ≥ 2 calls. Max-per-fn shared (8).
- **D.3:** composition LICM → V3 → D217.
- **E.3:** conservative invalidation (any @F write skips all).
- **F.3:** closure-in-body skip; concurrent body skip; protocol
  receiver skip.

## 7. Edge case audit

- Recursive pure methods — semantically OK (deterministic).
- Generic pure methods — registry keyed by base receiver type
  name. Static dispatch only (Nova has no virtual pure methods).
- Pure methods returning consume-types — V3 skip (D131 ownership).
  Pure method by D24 is Pure → no effects → returns plain value.
  Consume-returning pure methods technically odd; defer V3.

## 8. Closure of Ф.0

DECISIONs A.3-F.3 finalized. Pure method registry approach simple
(single-pass). Composition с D217+D218 clean. Conservative
invalidation в V3, refinement в V3.1 followup. Ready Ф.1+Ф.2.
