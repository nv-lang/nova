//! Ф.3.3 (Plan 33.6): pattern-aware suggested fixes для failed verification.
//!
//! Принимает AST-выражение body/contract и emit actionable suggestions
//! когда находит обычные mistake-patterns. AI-friendly format.
//!
//! Поддерживаемые patterns (V1):
//! 1. `x / k` или `x % k` без `requires k != 0`.
//! 2. `arr[i]` без `requires i >= 0 && i < arr.len()`.
//! 3. `decreases n + 1` (positive delta) — должен убывать.

use crate::ast::*;

/// Public entry-point. Возвращает list of suggestion strings.
pub fn suggest_fixes(fd: &FnDecl) -> Vec<String> {
    let mut suggestions = Vec::new();
    let req_text = collect_requires_text(&fd.contracts);
    walk_body(&fd.body, &req_text, &mut suggestions);
    // FnDecl.decreases — отдельное поле (не ContractKind).
    if let Some(dec_expr) = &fd.decreases {
        check_decreases_direction(dec_expr, &mut suggestions);
    }
    // Dedup.
    suggestions.sort();
    suggestions.dedup();
    suggestions
}

fn collect_requires_text(contracts: &[Contract]) -> String {
    let mut out = String::new();
    for c in contracts {
        if !matches!(c.kind, ContractKind::Requires) { continue; }
        out.push_str(&format!("{:?} ", c.expr.kind));
    }
    out
}

fn walk_body(body: &FnBody, req: &str, out: &mut Vec<String>) {
    match body {
        FnBody::Expr(e) => walk_expr(e, req, out),
        FnBody::Block(b) => {
            for s in &b.stmts {
                walk_stmt(s, req, out);
            }
            if let Some(e) = &b.trailing {
                walk_expr(e, req, out);
            }
        }
        FnBody::External => {}
    }
}

fn walk_stmt(s: &Stmt, req: &str, out: &mut Vec<String>) {
    match s {
        Stmt::Let(l) => walk_expr(&l.value, req, out),
        Stmt::Expr(e) => walk_expr(e, req, out),
        Stmt::Assign { value, .. } => walk_expr(value, req, out),
        _ => {}
    }
}

fn walk_expr(e: &Expr, req: &str, out: &mut Vec<String>) {
    match &e.kind {
        // Pattern 1: x / k или x % k без requires k != 0.
        ExprKind::Binary { op, right, .. } if matches!(op, BinOp::Div | BinOp::Mod) => {
            if let ExprKind::Ident(divisor) = &right.kind {
                let already_safe = req.contains(&format!("\"{}\"", divisor))
                    || req.contains(&format!("Ident({}", divisor));
                if !already_safe {
                    out.push(format!(
                        "hint: добавьте `requires {} != 0` для безопасности делителя",
                        divisor));
                }
            }
            // Recurse в left.
            if let ExprKind::Binary { left, right, .. } = &e.kind {
                walk_expr(left, req, out);
                walk_expr(right, req, out);
            }
        }
        // Pattern 2: arr[i] без bounds check.
        ExprKind::Index { obj, index } => {
            if let (ExprKind::Ident(arr), ExprKind::Ident(idx)) = (&obj.kind, &index.kind) {
                if !req.contains(&format!("{}.len", arr)) {
                    out.push(format!(
                        "hint: для `{}[{}]` добавьте `requires {} >= 0 && {} < {}.len()`",
                        arr, idx, idx, idx, arr));
                }
            }
            walk_expr(obj, req, out);
            walk_expr(index, req, out);
        }
        ExprKind::Binary { left, right, .. } => {
            walk_expr(left, req, out);
            walk_expr(right, req, out);
        }
        ExprKind::Unary { operand, .. } => walk_expr(operand, req, out),
        ExprKind::Call { func, args, .. } => {
            walk_expr(func, req, out);
            for a in args { walk_expr(a.expr(), req, out); }
        }
        ExprKind::If { cond, then, else_ } => {
            walk_expr(cond, req, out);
            for s in &then.stmts {
                walk_stmt(s, req, out);
            }
            if let Some(t) = &then.trailing {
                walk_expr(t, req, out);
            }
            if let Some(eb) = else_ {
                match eb {
                    ElseBranch::Block(b) => {
                        for s in &b.stmts {
                            walk_stmt(s, req, out);
                        }
                        if let Some(t) = &b.trailing {
                            walk_expr(t, req, out);
                        }
                    }
                    ElseBranch::If(e) => walk_expr(e, req, out),
                }
            }
        }
        _ => {}
    }
}

/// Pattern 3: `decreases n + 1` (positive delta) → должен убывать.
fn check_decreases_direction(dec_expr: &Expr, out: &mut Vec<String>) {
    if let ExprKind::Binary { op: BinOp::Add, right, .. } = &dec_expr.kind {
        if let ExprKind::IntLit(n) = &right.kind {
            if *n > 0 {
                out.push(format!(
                    "hint: `decreases X + {}` — measure должен убывать, не возрастать. \
                     Используйте `decreases X - {}` или другую убывающую meri.",
                    n, n));
            }
        }
    }
}
