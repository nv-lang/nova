// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 123.2 Ф.0 — Investigation + DECISIONs A.2-F.2

> **Дата:** 2026-06-02.

---

## 1. Loop AST forms

Per `compiler-codegen/src/ast/mod.rs`:
- `ExprKind::For { pattern, iter, body, elem_type, invariants, decreases, iter_consume }`
- `ExprKind::ParallelFor { pattern, iter, body, elem_type }`
- `ExprKind::While { cond, body, invariants, decreases }`
- `ExprKind::WhileLet { pattern, scrutinee, body, invariants, decreases }`
- `ExprKind::Loop { body, invariants, decreases }`

All have a `body: Block`. ParallelFor — concurrent body execution
(skip LICM polностью because of aliasing).

## 2. Composition with Plan 123.1

Strategy: LICM phase runs in `cache_module` **first**, before per-fn
ro/mut caching.

For each FnDecl с receiver:
1. Build registry (already done in Plan 123.1).
2. **LICM phase:** walk fn body, find Loops, for each loop:
   - Collect @F reads inside.
   - Detect mutations inside.
   - For eligible fields → emit hoist `ro _at_<F>_loop = @<F>` in
     enclosing block immediately before loop, replace reads inside
     loop with cache ident.
3. **Plan 123.1 phase:** standard analysis + ro/mut prefix caching.
   By now, @F reads inside loops already replaced — counted only
   reads outside loops + reads NOT eligible for LICM.

## 3. Loop hoisting placement

For loop appearing as a `Stmt::Expr(loop_expr)` in some Block:
- Insert hoist `Stmt::Let(...)` immediately before the loop's Stmt.

For loop appearing as `Block.trailing` (e.g. `{ stmts; loop_expr }`):
- Append hoist to `Block.stmts`, leave loop as trailing.

For loop appearing nested in expressions (e.g. `if cond { for ... }`):
- The loop is itself the trailing of an inner Block (e.g. `then` Block
  of If). Insert hoist into that Block.

For loop appearing as FnBody (rare — `fn foo() => for ...`):
- Convert to Block body, insert hoist, then loop.

## 4. Invariance detection

For each field F and each loop body:
- `has_mutation_to(body, F)` — recursive walk, finds `Assign { target:
  Member{SelfAccess, F}, ... }` or compound assigns.
- `has_call(body)` — V2 conservative — any Call invalidates mut.
- `closure_captures(body, F)` — recursive walk, find closure body refs.
- `has_spawn(body)` — Spawn/Supervised/Detach/Blocking treated as
  unsafe for hoist.

Eligibility per field:
- ro field: NOT has_mutation_to + NOT closure_captures + NOT has_spawn.
- mut field: NOT has_mutation_to + NOT has_call + NOT closure_captures
  + NOT has_spawn.

## 5. DECISIONs A.2-F.2

- **DECISION-A.2 (LICM scope):** for/while/loop/while_let. Skip
  ParallelFor (concurrent body).
- **DECISION-B.2 (placement):** immediately before loop in enclosing
  block. Naming `_at_<F>_loop` (distinct from `_at_<F>`).
- **DECISION-C.2 (threshold):** ≥2 reads inside loop. Max-per-loop
  cap = 4. Total fn cap (8) shared with Plan 123.1.
- **DECISION-D.2 (composition):** LICM before Plan 123.1 per-fn pass.
- **DECISION-E.2 (V2 scope):** receiver-field loads only. Pure-call
  hoisting → Plan 123.3.
- **DECISION-F.2 (edge cases):** nested loops — V2 conservative
  hoist to immediate loop's enclosing block, не cross multiple
  levels; closure-in-loop → skip; spawn/parallel → skip.

## 6. Risk mitigation

- R-2.1 covered: any Call → skip mut hoist; ro hoist OK (frozen).
- R-2.2 covered: hoisted load side-effect-free for record-fields
  (pointer derefs).
- R-2.3 covered: LICM runs first.

## 7. Closure of Ф.0

DECISIONs A.2-F.2 finalized. Composition strategy с Plan 123.1
clear. Loop body invariance detection helpers identified.
Ready Ф.1+Ф.2 merged implementation.
