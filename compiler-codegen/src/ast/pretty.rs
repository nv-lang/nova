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
        ExprKind::MapLit { elems, .. } => {
                let pairs = crate::ast::MapElem::cloned_pairs(&elems);
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
            // Plan 45 Ф.29.2: precedence-aware parens. Omit parens когда child
            // имеет higher precedence чем parent (или equal на правильной side
            // для associativity).
            let my_prec = binop_prec(op);
            // Left assoc: skip parens на left если child_prec >= my_prec.
            let left_needs = needs_paren_left(left, my_prec);
            let right_needs = needs_paren_right(right, my_prec);
            if left_needs { out.push('('); }
            write_expr(out, left);
            if left_needs { out.push(')'); }
            out.push(' ');
            out.push_str(binop_str(op));
            out.push(' ');
            if right_needs { out.push('('); }
            write_expr(out, right);
            if right_needs { out.push(')'); }
        }
        ExprKind::Unary { op, operand } => {
            // Unary имеет высокий precedence; operand parens нужны если
            // он binary с lower prec.
            let needs = matches!(operand.kind, ExprKind::Binary { .. });
            out.push_str(unop_str(op));
            if needs { out.push('('); }
            write_expr(out, operand);
            if needs { out.push(')'); }
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

/// Plan 45 Ф.29.2 — precedence levels по Nova syntax priority.
/// Higher number = tighter binding.
fn binop_prec(op: &BinOp) -> u8 {
    match op {
        BinOp::Mul | BinOp::Div | BinOp::Mod => 12,
        BinOp::Add | BinOp::Sub => 11,
        BinOp::Shl | BinOp::Shr => 10,
        BinOp::BitAnd => 9,
        BinOp::BitXor => 8,
        BinOp::BitOr => 7,
        BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => 6,
        BinOp::Eq | BinOp::Neq => 5,
        BinOp::And => 4,
        BinOp::Or => 3,
        BinOp::Implies => 2,
        BinOp::Iff => 1,
    }
}

/// Left side: skip parens если child_prec > my_prec (strictly higher),
/// или child_prec == my_prec (left-assoc default — already binds correctly).
fn needs_paren_left(child: &Expr, my_prec: u8) -> bool {
    if let ExprKind::Binary { op, .. } = &child.kind {
        binop_prec(op) < my_prec
    } else {
        false
    }
}

/// Right side: skip parens если child_prec > my_prec (strictly higher).
/// При equal — нужны parens (left-assoc; `a - b - c` ≠ `a - (b - c)`).
fn needs_paren_right(child: &Expr, my_prec: u8) -> bool {
    if let ExprKind::Binary { op, .. } = &child.kind {
        binop_prec(op) <= my_prec
    } else {
        false
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
    fn binary_op_no_paren_with_literals() {
        // Plan 45 Ф.29.2: precedence-aware — literal/ident operands не нуждаются в parens.
        let lhs = e(ExprKind::Ident("x".to_string()));
        let rhs = e(ExprKind::IntLit(1));
        let bin = e(ExprKind::Binary {
            op: BinOp::Add,
            left: Box::new(lhs),
            right: Box::new(rhs),
        });
        assert_eq!(print_expr(&bin), "x + 1");
    }

    #[test]
    fn comparison_gt_no_paren() {
        let bin = e(ExprKind::Binary {
            op: BinOp::Gt,
            left: Box::new(e(ExprKind::Ident("x".to_string()))),
            right: Box::new(e(ExprKind::IntLit(0))),
        });
        assert_eq!(print_expr(&bin), "x > 0");
    }

    #[test]
    fn higher_prec_no_paren() {
        // a + b * c — `*` имеет higher prec → нет parens вокруг `b * c`.
        let mul = e(ExprKind::Binary {
            op: BinOp::Mul,
            left: Box::new(e(ExprKind::Ident("b".to_string()))),
            right: Box::new(e(ExprKind::Ident("c".to_string()))),
        });
        let add = e(ExprKind::Binary {
            op: BinOp::Add,
            left: Box::new(e(ExprKind::Ident("a".to_string()))),
            right: Box::new(mul),
        });
        assert_eq!(print_expr(&add), "a + b * c");
    }

    #[test]
    fn lower_prec_needs_paren() {
        // (a + b) * c — `+` имеет lower prec чем `*` parent → parens нужны.
        let add = e(ExprKind::Binary {
            op: BinOp::Add,
            left: Box::new(e(ExprKind::Ident("a".to_string()))),
            right: Box::new(e(ExprKind::Ident("b".to_string()))),
        });
        let mul = e(ExprKind::Binary {
            op: BinOp::Mul,
            left: Box::new(add),
            right: Box::new(e(ExprKind::Ident("c".to_string()))),
        });
        assert_eq!(print_expr(&mul), "(a + b) * c");
    }

    #[test]
    fn same_prec_right_needs_paren() {
        // a - (b - c) — right side same prec, не left-assoc → parens нужны.
        let inner = e(ExprKind::Binary {
            op: BinOp::Sub,
            left: Box::new(e(ExprKind::Ident("b".to_string()))),
            right: Box::new(e(ExprKind::Ident("c".to_string()))),
        });
        let outer = e(ExprKind::Binary {
            op: BinOp::Sub,
            left: Box::new(e(ExprKind::Ident("a".to_string()))),
            right: Box::new(inner),
        });
        assert_eq!(print_expr(&outer), "a - (b - c)");
    }

    #[test]
    fn same_prec_left_no_paren() {
        // (a - b) - c — left side same prec, left-assoc default → no parens.
        let inner = e(ExprKind::Binary {
            op: BinOp::Sub,
            left: Box::new(e(ExprKind::Ident("a".to_string()))),
            right: Box::new(e(ExprKind::Ident("b".to_string()))),
        });
        let outer = e(ExprKind::Binary {
            op: BinOp::Sub,
            left: Box::new(inner),
            right: Box::new(e(ExprKind::Ident("c".to_string()))),
        });
        assert_eq!(print_expr(&outer), "a - b - c");
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
