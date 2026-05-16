//! Plan 45 Ф.28.1 — AST pretty-printer shared util (expression-focused MVP).
//!
//! Реконструирует Nova source для **expressions** из AST. Используется в:
//! - `doc/collector::render_expr` (для contract expressions в JSON/MD)
//!
//! **Scope:** expressions только (не statements, не types — для них есть
//! existing renderers в diag/collector). Это focused replacement для
//! `collector::render_expr` который имел `_ => "..."` placeholder.
//!
//! **Design goals:**
//! - **Полное покрытие ExprKind** — каждый variant явный branch, fallback
//!   `<kind>` с именем variant'а (не anonymous `...`).
//! - **Compact** — no extra spaces beyond syntactical requirement.
//! - **Round-trippable где возможно** для простых expressions.
//!
//! **Limitations:**
//! - Не сохраняет comments/whitespace (AST→source, не CST).
//! - Binary ops parenthesized всегда (slight redundancy для safety).
//! - Complex stmts (Match arms, For/While bodies) — `{ ... }` placeholder.

use super::*;
use std::fmt::Write;

/// Plan 45 Ф.28.1 — entry point: pretty-print expression в Nova source.
pub fn print_expr(e: &Expr) -> String {
    let mut out = String::new();
    write_expr(&mut out, e);
    out
}

fn write_expr(out: &mut String, e: &Expr) {
    match &e.kind {
        ExprKind::IntLit(n) => { let _ = write!(out, "{}", n); }
        ExprKind::FloatLit(f) => { let _ = write!(out, "{}", f); }
        ExprKind::BoolLit(b) => { let _ = write!(out, "{}", b); }
        ExprKind::StrLit(s) => {
            let _ = write!(out, "\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""));
        }
        ExprKind::CharLit(c) => {
            let _ = write!(out, "'{}'", char::from_u32(*c).unwrap_or('?'));
        }
        ExprKind::UnitLit => out.push_str("()"),
        ExprKind::InterpolatedStr { parts } => {
            out.push('"');
            for p in parts {
                match p {
                    InterpStrPart::Lit(s) => out.push_str(s),
                    InterpStrPart::Expr(e) => {
                        out.push_str("${");
                        write_expr(out, e);
                        out.push('}');
                    }
                }
            }
            out.push('"');
        }
        ExprKind::ArrayLit(elems) => {
            out.push('[');
            for (i, el) in elems.iter().enumerate() {
                if i > 0 { out.push_str(", "); }
                match el {
                    ArrayElem::Item(x) => write_expr(out, x),
                    ArrayElem::Spread(x) => {
                        out.push_str("...");
                        write_expr(out, x);
                    }
                }
            }
            out.push(']');
        }
        ExprKind::MapLit { pairs, .. } => {
            out.push('[');
            for (i, (k, v)) in pairs.iter().enumerate() {
                if i > 0 { out.push_str(", "); }
                write_expr(out, k);
                out.push_str(": ");
                write_expr(out, v);
            }
            out.push(']');
        }
        ExprKind::RecordLit { type_name, fields, .. } => {
            if let Some(p) = type_name {
                out.push_str(&p.join("."));
                out.push(' ');
            }
            out.push_str("{ ");
            for (i, f) in fields.iter().enumerate() {
                if i > 0 { out.push_str(", "); }
                if f.is_spread {
                    out.push_str("...");
                    if let Some(v) = &f.value { write_expr(out, v); }
                } else {
                    out.push_str(&f.name);
                    if let Some(v) = &f.value {
                        out.push_str(": ");
                        write_expr(out, v);
                    }
                }
            }
            out.push_str(" }");
        }
        ExprKind::TupleLit(xs) => {
            out.push('(');
            for (i, x) in xs.iter().enumerate() {
                if i > 0 { out.push_str(", "); }
                write_expr(out, x);
            }
            out.push(')');
        }
        ExprKind::Ident(name) => out.push_str(name),
        ExprKind::Path(parts) => out.push_str(&parts.join(".")),
        ExprKind::SelfAccess => out.push('@'),
        ExprKind::Member { obj, name } => {
            write_expr(out, obj);
            out.push('.');
            out.push_str(name);
        }
        ExprKind::Index { obj, index } => {
            write_expr(out, obj);
            out.push('[');
            write_expr(out, index);
            out.push(']');
        }
        ExprKind::TurboFish { base, type_args: _ } => {
            // Type args rendered minimal — single brackets с placeholder.
            write_expr(out, base);
            out.push_str("[..]");
        }
        ExprKind::Call { func, args, .. } => {
            write_expr(out, func);
            out.push('(');
            for (i, a) in args.iter().enumerate() {
                if i > 0 { out.push_str(", "); }
                write_call_arg(out, a);
            }
            out.push(')');
        }
        ExprKind::Binary { op, left, right } => {
            out.push('(');
            write_expr(out, left);
            out.push(' ');
            out.push_str(binop_str(op));
            out.push(' ');
            write_expr(out, right);
            out.push(')');
        }
        ExprKind::Unary { op, operand } => {
            out.push_str(unop_str(op));
            write_expr(out, operand);
        }
        ExprKind::If { cond, .. } => {
            // Body — Block; render compact placeholder.
            out.push_str("if ");
            write_expr(out, cond);
            out.push_str(" { ... }");
        }
        ExprKind::Range { start, end, inclusive } => {
            write_expr(out, start);
            out.push_str(if *inclusive { "..=" } else { ".." });
            write_expr(out, end);
        }
        ExprKind::Forall { var, range, body } => {
            let _ = write!(out, "forall {} in ", var);
            write_expr(out, range);
            out.push_str(" : ");
            write_expr(out, body);
        }
        ExprKind::Interrupt(opt) => {
            out.push_str("interrupt");
            if let Some(e) = opt {
                out.push(' ');
                write_expr(out, e);
            }
        }
        // Fallback с kind name — explicit, не anonymous "...".
        _ => {
            out.push('<');
            out.push_str(expr_kind_name(&e.kind));
            out.push('>');
        }
    }
}

fn write_call_arg(out: &mut String, a: &CallArg) {
    match a {
        CallArg::Item(x) => write_expr(out, x),
        CallArg::Spread(x) => {
            out.push_str("...");
            write_expr(out, x);
        }
        CallArg::Named { name, value } => {
            out.push_str(name);
            out.push_str(": ");
            write_expr(out, value);
        }
    }
}

fn binop_str(op: &BinOp) -> &'static str {
    match op {
        BinOp::Add => "+", BinOp::Sub => "-", BinOp::Mul => "*",
        BinOp::Div => "/", BinOp::Mod => "%",
        BinOp::Eq => "==", BinOp::Neq => "!=",
        BinOp::Lt => "<", BinOp::Le => "<=",
        BinOp::Gt => ">", BinOp::Ge => ">=",
        BinOp::And => "&&", BinOp::Or => "||",
        BinOp::Implies => "==>", BinOp::Iff => "<==>",
        BinOp::BitAnd => "&", BinOp::BitOr => "|", BinOp::BitXor => "^",
        BinOp::Shl => "<<", BinOp::Shr => ">>",
    }
}

fn unop_str(op: &UnOp) -> &'static str {
    match op {
        UnOp::Neg => "-",
        UnOp::Not => "!",
    }
}

fn expr_kind_name(k: &ExprKind) -> &'static str {
    match k {
        ExprKind::Match { .. } => "match",
        ExprKind::IfLet { .. } => "if-let",
        ExprKind::For { .. } => "for",
        ExprKind::ParallelFor { .. } => "parallel-for",
        ExprKind::While { .. } => "while",
        ExprKind::WhileLet { .. } => "while-let",
        ExprKind::Loop { .. } => "loop",
        ExprKind::Select { .. } => "select",
        ExprKind::Lambda { .. } => "lambda",
        ExprKind::ClosureLight { .. } => "closure",
        ExprKind::ClosureFull(_) => "closure-full",
        ExprKind::With { .. } => "with",
        ExprKind::HandlerLit { .. } => "handler-lit",
        ExprKind::Forbid { .. } => "forbid-block",
        ExprKind::Realtime { .. } => "realtime-block",
        ExprKind::Exists { .. } => "exists",
        _ => "expr",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diag::Span;

    fn span() -> Span { Span { start: 0, end: 0, file_id: 0 } }
    fn e(kind: ExprKind) -> Expr { Expr { kind, span: span() } }

    #[test]
    fn literal_int() {
        assert_eq!(print_expr(&e(ExprKind::IntLit(42))), "42");
    }

    #[test]
    fn literal_bool() {
        assert_eq!(print_expr(&e(ExprKind::BoolLit(true))), "true");
        assert_eq!(print_expr(&e(ExprKind::BoolLit(false))), "false");
    }

    #[test]
    fn ident() {
        assert_eq!(print_expr(&e(ExprKind::Ident("x".to_string()))), "x");
    }

    #[test]
    fn binary_op_paren() {
        let lhs = e(ExprKind::Ident("x".to_string()));
        let rhs = e(ExprKind::IntLit(1));
        let bin = e(ExprKind::Binary {
            op: BinOp::Add,
            left: Box::new(lhs),
            right: Box::new(rhs),
        });
        assert_eq!(print_expr(&bin), "(x + 1)");
    }

    #[test]
    fn comparison_gt() {
        let bin = e(ExprKind::Binary {
            op: BinOp::Gt,
            left: Box::new(e(ExprKind::Ident("x".to_string()))),
            right: Box::new(e(ExprKind::IntLit(0))),
        });
        assert_eq!(print_expr(&bin), "(x > 0)");
    }

    #[test]
    fn member_access() {
        let mem = e(ExprKind::Member {
            obj: Box::new(e(ExprKind::Ident("obj".to_string()))),
            name: "field".to_string(),
        });
        assert_eq!(print_expr(&mem), "obj.field");
    }

    #[test]
    fn no_anonymous_placeholder_in_combined() {
        let left = e(ExprKind::Binary {
            op: BinOp::Gt,
            left: Box::new(e(ExprKind::Ident("x".to_string()))),
            right: Box::new(e(ExprKind::IntLit(0))),
        });
        let right = e(ExprKind::Binary {
            op: BinOp::Lt,
            left: Box::new(e(ExprKind::Ident("y".to_string()))),
            right: Box::new(e(ExprKind::IntLit(100))),
        });
        let combined = e(ExprKind::Binary {
            op: BinOp::And,
            left: Box::new(left),
            right: Box::new(right),
        });
        let r = print_expr(&combined);
        assert!(!r.contains("..."), "pretty не должна иметь '...' placeholder в binary, got: {}", r);
        assert!(r.contains("x > 0"));
        assert!(r.contains("y < 100"));
        assert!(r.contains("&&"));
    }

    #[test]
    fn fallback_is_kind_name_not_dot_dot_dot() {
        // Block expression (без handler) → <expr> или specific kind name.
        // Просто verify что result не "..." anonymous.
        // У нас нет easy way создать Match без big setup, проверим через
        // что fallback вообще emits "<...>" pattern.
        let kind_name = expr_kind_name(&ExprKind::Match { scrutinee: Box::new(e(ExprKind::IntLit(0))), arms: vec![] });
        assert_eq!(kind_name, "match");
    }
}
