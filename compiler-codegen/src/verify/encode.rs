//! Plan 33.1 Ф.3: Nova AST → SMT-IR encoder.
//!
//! Поддерживает straight-line код без mut/циклов (33.1 scope):
//! - Literals: int, bool, str.
//! - Variables (parameters, `result`, `old(...)`).
//! - Binary ops: +/-/*///%, ==/!=/<,<=,>,>=, &&/||, ==>/<==>.
//! - Unary: !, -.
//! - if/then/else (encoded as ite via and/or/impl).
//!
//! Не поддерживает (даёт `EncodingError`):
//! - field access (record types — uninterpreted в Ф.3, 33.2 расширит).
//! - method calls.
//! - other expressions (block, match, lambda, ...).

use crate::ast::{Expr, ExprKind, BinOp, UnOp};
use super::ir::*;

#[derive(Debug, Clone)]
pub enum EncodingError {
    /// Конструкция не поддерживается trivial-encoder'ом 33.1.
    Unsupported(String),
}

/// Encode Nova-expr в SMT-term.
pub fn encode_expr(e: &Expr) -> Result<SmtTerm, EncodingError> {
    match &e.kind {
        ExprKind::IntLit(n) => Ok(SmtTerm::IntLit(*n)),
        ExprKind::BoolLit(b) => Ok(SmtTerm::BoolLit(*b)),
        ExprKind::StrLit(s) => Ok(SmtTerm::StrLit(s.clone())),
        ExprKind::Ident(n) => Ok(SmtTerm::Var(n.clone())),

        // `old(e)` — magic call. Encoder подменяет на fresh var `_old_<encoded>`.
        // В pipeline это значение equated с `encode(e) at entry-state`.
        ExprKind::Call { func, args, trailing } => {
            if trailing.is_none() && args.len() == 1 {
                if let ExprKind::Ident(name) = &func.kind {
                    if name == "old" {
                        let inner = encode_expr(args[0].expr())?;
                        // Name based on pretty-print для стабильности.
                        let key = format!("_old_{}", sanitize_for_var(&inner.pretty()));
                        return Ok(SmtTerm::Var(key));
                    }
                }
            }
            Err(EncodingError::Unsupported(format!(
                "call expressions in contracts not yet supported in Plan 33.1 \
                 (Plan 33.2 composition with `#pure` functions)"
            )))
        }

        ExprKind::Binary { op, left, right } => {
            let l = encode_expr(left)?;
            let r = encode_expr(right)?;
            let op_str = bin_op_to_smt(*op)?;
            Ok(SmtTerm::App(op_str.into(), vec![l, r]))
        }

        ExprKind::Unary { op, operand } => {
            let v = encode_expr(operand)?;
            match op {
                UnOp::Not => Ok(SmtTerm::App("not".into(), vec![v])),
                UnOp::Neg => Ok(SmtTerm::App("-".into(),
                    vec![SmtTerm::IntLit(0), v])),
            }
        }

        // `if cond { then } else { else_ }` → ite(cond, then, else)
        // через `(or (and cond then) (and (not cond) else))`.
        ExprKind::If { cond, then, else_ } => {
            let cond_term = encode_expr(cond)?;
            // Block must be single expression for trivial encoding.
            if !then.stmts.is_empty() { return Err(EncodingError::Unsupported(
                "if-branch with statements not supported in trivial encoder".into())); }
            let then_term = match &then.trailing {
                Some(e) => encode_expr(e)?,
                None => return Err(EncodingError::Unsupported(
                    "if without trailing expression".into())),
            };
            let else_term = match else_ {
                Some(crate::ast::ElseBranch::Block(b)) => {
                    if !b.stmts.is_empty() { return Err(EncodingError::Unsupported(
                        "else-branch with statements not supported".into())); }
                    match &b.trailing {
                        Some(e) => encode_expr(e)?,
                        None => return Err(EncodingError::Unsupported(
                            "else-block without trailing".into())),
                    }
                }
                Some(crate::ast::ElseBranch::If(e)) => encode_expr(e)?,
                None => return Err(EncodingError::Unsupported(
                    "if without else not supported".into())),
            };
            // ite via or+and: (cond ∧ then) ∨ (¬cond ∧ else)
            Ok(SmtTerm::or(vec![
                SmtTerm::and(vec![cond_term.clone(), then_term]),
                SmtTerm::and(vec![SmtTerm::not(cond_term), else_term]),
            ]))
        }

        ExprKind::UnitLit => Ok(SmtTerm::Var("_unit".into())),

        // Member access (record fields) — uninterpreted в 33.1.
        // Кодируем как UF: `field_x(obj)`.
        ExprKind::Member { obj, name } => {
            let obj_t = encode_expr(obj)?;
            Ok(SmtTerm::App(format!("_field_{}", name), vec![obj_t]))
        }

        _ => Err(EncodingError::Unsupported(format!(
            "expression kind not supported in contract encoder (33.1)"
        ))),
    }
}

fn bin_op_to_smt(op: BinOp) -> Result<&'static str, EncodingError> {
    Ok(match op {
        BinOp::Add => "+", BinOp::Sub => "-",
        BinOp::Mul => "*", BinOp::Div => "/", BinOp::Mod => "%",
        BinOp::Eq => "=", BinOp::Neq => "!=",
        BinOp::Lt => "<", BinOp::Le => "<=",
        BinOp::Gt => ">", BinOp::Ge => ">=",
        BinOp::And => "and", BinOp::Or => "or",
        BinOp::Implies => "=>",
        BinOp::Iff => "=",
        BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor | BinOp::Shl | BinOp::Shr => {
            return Err(EncodingError::Unsupported(
                "bitwise operators in contracts not supported in Plan 33.1".into()));
        }
    })
}

/// Make valid SMT-IR var name from pretty-printed term.
fn sanitize_for_var(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_alphanumeric() || c == '_' { c } else { '_' })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diag::Span;

    fn span() -> Span { Span::new(0, 0) }

    fn ident(n: &str) -> Expr {
        Expr::new(ExprKind::Ident(n.into()), span())
    }

    fn int(n: i64) -> Expr { Expr::new(ExprKind::IntLit(n), span()) }

    fn bin(op: BinOp, l: Expr, r: Expr) -> Expr {
        Expr::new(ExprKind::Binary { op, left: Box::new(l), right: Box::new(r) }, span())
    }

    #[test]
    fn encode_simple_eq() {
        // x == 5
        let e = bin(BinOp::Eq, ident("x"), int(5));
        let t = encode_expr(&e).unwrap();
        assert_eq!(t,
            SmtTerm::App("=".into(),
                vec![SmtTerm::Var("x".into()), SmtTerm::IntLit(5)]));
    }

    #[test]
    fn encode_arith() {
        // x + 1
        let e = bin(BinOp::Add, ident("x"), int(1));
        let t = encode_expr(&e).unwrap();
        assert_eq!(t,
            SmtTerm::App("+".into(),
                vec![SmtTerm::Var("x".into()), SmtTerm::IntLit(1)]));
    }

    #[test]
    fn encode_implication() {
        // x > 0 ==> x >= 1
        let e = bin(BinOp::Implies,
            bin(BinOp::Gt, ident("x"), int(0)),
            bin(BinOp::Ge, ident("x"), int(1)));
        let t = encode_expr(&e).unwrap();
        // Just check op is "=>" — структура была.
        match t {
            SmtTerm::App(op, _) => assert_eq!(op, "=>"),
            _ => panic!(),
        }
    }
}
