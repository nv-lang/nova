// SPDX-License-Identifier: MIT OR Apache-2.0
//! Plan 174 (D409, owner amendment 2026-07-06): `-> @` auto-return lowering.
//!
//! ## What
//!
//! D409 flips fluent-return (`-> @`, D132) от manual to fully automatic:
//! explicit `@`/`return @`/`=> @` are now a compile error
//! (`E_EXPLICIT_SELF_RETURN`, enforced by `types::check_fluent_return` on
//! the AS-WRITTEN source, BEFORE this pass runs). Every OTHER exit из a
//! `-> @` body must yield the receiver at runtime:
//!
//!   - end of body with no trailing expression → `@`;
//!   - bare `return` (no value) → `return @`;
//!   - a trailing expression that does NOT itself resolve to the receiver
//!     (i.e. not a call chain into another `-> @` method) → discard it as a
//!     statement, then `@` (e.g. `{ @buf.push(v) }` → the `push` result,
//!     `Vec[u8]*`, is discarded; the `WriteBuffer` receiver is returned).
//!
//! This module performs that lowering by AST rewrite, reusing the EXACT
//! same `@` / `return @` shapes the codegen already emits for the
//! pre-D409 manual form (`emit_c.rs` Stmt::Return / block-trailing paths) —
//! no new codegen is needed, only synthesized `ExprKind::SelfAccess` nodes
//! wherever an implicit exit needs one.
//!
//! ## Why AST-level (not codegen-level)
//!
//! `emit_c.rs` has half a dozen near-duplicate fn-body emitters (mono/
//! non-mono/ensures/contracts/closures/…); patching every one of them to
//! special-case `-> @` auto-return would multiply surface area for a
//! single, purely syntactic transform. Rewriting the AST once, right after
//! `check_module` gates on `E_EXPLICIT_SELF_RETURN`, means every codegen
//! path just sees an (already legal, pre-D409) explicit `@`/`return @`
//! shape — zero codegen changes required (§0).
//!
//! ## Recursion scope
//!
//! `return` is a STATEMENT (parser: `parse_stmt_or_expr`'s `KwReturn` arm)
//! — it can only appear inside a `Block`, never nested inside an arbitrary
//! sub-expression (`Binary`/`Call`/`Member`/…). So the bare-`return`
//! rewrite only needs to follow control-flow constructs that carry a
//! `Block`: `If`/`IfLet`/`Match`/`While`/`WhileLet`/`For`/`ParallelFor`/
//! `Loop`/`Block`-expr/`ConsumeScope`. It does NOT recurse into closures/
//! lambdas — those are a separate return-scope, unaffected by the
//! enclosing `-> @` method's auto-return.
//!
//! Trailing-expression wrapping (the "discard non-`@` value, return
//! receiver instead" rewrite) only applies at the OUTERMOST fn-body level:
//! a nested block's own trailing (e.g. an `if` used as a plain statement
//! partway through the body) is already discarded by ordinary statement
//! semantics and never reaches the function's return slot.

use crate::ast::*;
use crate::Span;

/// Entry point: mutate every `-> @` (returns_receiver) function/method body
/// in `module` (+ peer_files) so every implicit exit returns the receiver.
/// Call AFTER `types::check_module` (or an equivalent gate) has rejected
/// `E_EXPLICIT_SELF_RETURN` — the input is assumed already D409-legal.
pub fn lower_module(module: &mut Module) {
    for item in &mut module.items {
        lower_item(item);
    }
    for pf in &mut module.peer_files {
        for item in &mut pf.items_here {
            lower_item(item);
        }
    }
}

fn lower_item(item: &mut Item) {
    if let Item::Fn(f) = item {
        if f.returns_receiver {
            lower_fn_body(&mut f.body);
        }
    }
}

fn self_access(span: Span) -> Expr {
    Expr::new(ExprKind::SelfAccess, span)
}

fn lower_fn_body(body: &mut FnBody) {
    match body {
        FnBody::External => {}
        FnBody::Expr(e) => {
            rewrite_bare_returns_in_expr(e);
            // D409-legal input: `e` is never bare `@` here (checker already
            // rejected that as E_EXPLICIT_SELF_RETURN). Wrap into a Block so
            // the (possibly non-receiver-valued) arrow-body expression is
            // discarded as a statement and the receiver returned instead —
            // reusing the exact Block{stmts, trailing=Some(@)} shape the
            // manual pre-D409 form already used.
            let span = e.span;
            let taken = std::mem::replace(e, self_access(span));
            *body = FnBody::Block(Block {
                stmts: vec![Stmt::Expr(taken)],
                trailing: Some(Box::new(self_access(span))),
                span,
                is_unsafe: false,
            });
        }
        FnBody::Block(b) => lower_outer_block(b),
    }
}

/// Lowering for the fn's OUTERMOST block: rewrites nested bare returns
/// (recursive) AND normalizes the trailing position (missing / discarded).
fn lower_outer_block(b: &mut Block) {
    for stmt in &mut b.stmts {
        rewrite_bare_returns_in_stmt(stmt);
    }
    match &mut b.trailing {
        None => {
            b.trailing = Some(Box::new(self_access(b.span)));
        }
        Some(t) => {
            rewrite_bare_returns_in_expr(t);
            // D409-legal input: `t` is never bare `@` (checker-rejected).
            // Discard its (non-receiver) value as a statement, return `@`.
            let span = t.span;
            let taken = std::mem::replace(t.as_mut(), self_access(span));
            b.stmts.push(Stmt::Expr(taken));
        }
    }
}

/// Rewrite nested bare `return` → `return @` inside a block reached through
/// control flow (NOT the fn's own trailing-wrapping — that's `lower_outer_block`
/// only, see module doc "Recursion scope").
fn lower_inner_block(b: &mut Block) {
    for stmt in &mut b.stmts {
        rewrite_bare_returns_in_stmt(stmt);
    }
    if let Some(t) = &mut b.trailing {
        rewrite_bare_returns_in_expr(t);
    }
}

fn rewrite_bare_returns_in_stmt(s: &mut Stmt) {
    match s {
        Stmt::Return { value, span } => {
            if value.is_none() {
                *value = Some(self_access(*span));
            }
        }
        Stmt::Let(decl) => rewrite_bare_returns_in_expr(&mut decl.value),
        Stmt::Assign { value, .. } => rewrite_bare_returns_in_expr(value),
        Stmt::TupleAssign { rhs, .. } => {
            for e in rhs { rewrite_bare_returns_in_expr(e); }
        }
        Stmt::ConsumeScope { body, .. } => lower_inner_block(body),
        Stmt::Expr(e) => rewrite_bare_returns_in_expr(e),
        _ => {}
    }
}

fn rewrite_bare_returns_in_expr(e: &mut Expr) {
    match &mut e.kind {
        ExprKind::If { then, else_, .. } => {
            lower_inner_block(then);
            if let Some(eb) = else_ {
                match eb {
                    ElseBranch::Block(b) => lower_inner_block(b),
                    ElseBranch::If(e2) => rewrite_bare_returns_in_expr(e2),
                }
            }
        }
        ExprKind::IfLet { then, else_, .. } => {
            lower_inner_block(then);
            if let Some(eb) = else_ {
                match eb {
                    ElseBranch::Block(b) => lower_inner_block(b),
                    ElseBranch::If(e2) => rewrite_bare_returns_in_expr(e2),
                }
            }
        }
        ExprKind::Match { arms, .. } => {
            for arm in arms {
                match &mut arm.body {
                    MatchArmBody::Block(b) => lower_inner_block(b),
                    MatchArmBody::Expr(e2) => rewrite_bare_returns_in_expr(e2),
                }
            }
        }
        ExprKind::While { body, .. } | ExprKind::Loop { body, .. } => lower_inner_block(body),
        ExprKind::WhileLet { body, .. } => lower_inner_block(body),
        ExprKind::For { body, .. } | ExprKind::ParallelFor { body, .. } => lower_inner_block(body),
        ExprKind::Block(b) => lower_inner_block(b),
        _ => {}
    }
}
