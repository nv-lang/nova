//! Lint-проходы по AST.
//!
//! Lint — это **warning**, не error: компилятор возвращает Diagnostic'и,
//! но компиляция продолжается. CLI решает выводить ли их (по умолчанию
//! да; `--no-lint` отключает). В отличие от parser/typecheck-error'ов,
//! lints программист может игнорировать.
//!
//! Текущие правила:
//!  - `export-fail-untyped`: `export fn ... Fail -> ...` без `[E]` —
//!    warning. Public API должен иметь typed Fail (D65 convention).

use crate::ast::{
    ArrayElem, Block, ClosureBody, ElseBranch, Expr, ExprKind, FnBody, FnDecl,
    HandlerMethodBody, Item, MatchArmBody, Module, Stmt, TypeDeclKind, TypeRef,
};
use crate::diag::{Diagnostic, Span};
use std::collections::HashSet;

/// Один lint-warning.
#[derive(Debug, Clone)]
pub struct LintWarning {
    pub rule: &'static str,
    pub diag: Diagnostic,
}

/// Прогон всех lint-проверок на модуле. Возвращает список warning'ов.
pub fn lint_module(m: &Module) -> Vec<LintWarning> {
    let mut warnings = Vec::new();
    let effect_names = collect_effect_names(m);
    let protocol_names = collect_protocol_names(m);
    for item in &m.items {
        match item {
            Item::Fn(f) => {
                check_fn(f, &mut warnings);
                check_protocol_in_effect_position(f, &protocol_names, &effect_names, &mut warnings);
                // Plan 52 Ф.2: map-литерал lints (dup-key, NaN-key) —
                // требуют обхода выражений внутри тела функции.
                match &f.body {
                    FnBody::Expr(e) => walk_expr_lints(e, &mut warnings),
                    FnBody::Block(b) => walk_block_lints(b, &mut warnings),
                    FnBody::External => {}
                }
            }
            Item::Test(t) => {
                walk_block_lints(&t.body, &mut warnings);
            }
            Item::Const(c) => walk_expr_lints(&c.value, &mut warnings),
            Item::Let(l) => walk_expr_lints(&l.value, &mut warnings),
            Item::Type(_) => {}
        }
    }
    warnings
}

/// Plan 52 Ф.2: рекурсивный обход блока для lint-проверок выражений.
fn walk_block_lints(b: &Block, out: &mut Vec<LintWarning>) {
    for s in &b.stmts {
        walk_stmt_lints(s, out);
    }
    if let Some(t) = &b.trailing {
        walk_expr_lints(t, out);
    }
}

fn walk_stmt_lints(s: &Stmt, out: &mut Vec<LintWarning>) {
    match s {
        Stmt::Expr(e) => walk_expr_lints(e, out),
        Stmt::Let(d) => walk_expr_lints(&d.value, out),
        Stmt::Assign { target, value, .. } => {
            walk_expr_lints(target, out);
            walk_expr_lints(value, out);
        }
        Stmt::Return { value, .. } => {
            if let Some(v) = value { walk_expr_lints(v, out); }
        }
        Stmt::Throw { value, .. } => walk_expr_lints(value, out),
        Stmt::Break(_) | Stmt::Continue(_) => {}
        Stmt::Defer { body, .. } | Stmt::ErrDefer { body, .. } => walk_expr_lints(body, out),
        Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => walk_expr_lints(expr, out),
    }
}

/// Plan 52 Ф.2: рекурсивный обход выражения. На каждом `MapLit` запускает
/// map-литерал lints; рекурсивно спускается во все под-выражения.
fn walk_expr_lints(e: &Expr, out: &mut Vec<LintWarning>) {
    if let ExprKind::MapLit(pairs) = &e.kind {
        check_map_literal_lints(pairs, out);
    }
    match &e.kind {
        ExprKind::MapLit(pairs) => {
            for (k, v) in pairs {
                walk_expr_lints(k, out);
                walk_expr_lints(v, out);
            }
        }
        ExprKind::ArrayLit(elems) => {
            for el in elems {
                match el {
                    ArrayElem::Item(x) | ArrayElem::Spread(x) => walk_expr_lints(x, out),
                }
            }
        }
        ExprKind::TupleLit(elems) => {
            for x in elems { walk_expr_lints(x, out); }
        }
        ExprKind::RecordLit { fields, .. } => {
            for f in fields {
                if let Some(v) = &f.value { walk_expr_lints(v, out); }
            }
        }
        ExprKind::Call { func, args, trailing } => {
            walk_expr_lints(func, out);
            for a in args { walk_expr_lints(a.expr(), out); }
            if let Some(t) = trailing {
                match t {
                    crate::ast::Trailing::Block(b) => walk_block_lints(b, out),
                    crate::ast::Trailing::LegacyBlockWithParams(tb) => walk_block_lints(&tb.body, out),
                    crate::ast::Trailing::Fn(sb) => match &sb.body {
                        FnBody::Expr(x) => walk_expr_lints(x, out),
                        FnBody::Block(b) => walk_block_lints(b, out),
                        FnBody::External => {}
                    },
                }
            }
        }
        ExprKind::TurboFish { base, .. } => walk_expr_lints(base, out),
        ExprKind::Try(x) | ExprKind::Bang(x) => walk_expr_lints(x, out),
        ExprKind::Coalesce(a, b) => { walk_expr_lints(a, out); walk_expr_lints(b, out); }
        ExprKind::As(x, _) | ExprKind::Is(x, _) => walk_expr_lints(x, out),
        ExprKind::Binary { left, right, .. } => {
            walk_expr_lints(left, out); walk_expr_lints(right, out);
        }
        ExprKind::Unary { operand, .. } => walk_expr_lints(operand, out),
        ExprKind::Member { obj, .. } => walk_expr_lints(obj, out),
        ExprKind::Index { obj, index } => {
            walk_expr_lints(obj, out); walk_expr_lints(index, out);
        }
        ExprKind::If { cond, then, else_ } => {
            walk_expr_lints(cond, out);
            walk_block_lints(then, out);
            if let Some(eb) = else_ {
                match eb {
                    ElseBranch::Block(b) => walk_block_lints(b, out),
                    ElseBranch::If(x) => walk_expr_lints(x, out),
                }
            }
        }
        ExprKind::IfLet { scrutinee, then, else_, .. } => {
            walk_expr_lints(scrutinee, out);
            walk_block_lints(then, out);
            if let Some(eb) = else_ {
                match eb {
                    ElseBranch::Block(b) => walk_block_lints(b, out),
                    ElseBranch::If(x) => walk_expr_lints(x, out),
                }
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            walk_expr_lints(scrutinee, out);
            for arm in arms {
                if let Some(g) = &arm.guard { walk_expr_lints(g, out); }
                match &arm.body {
                    MatchArmBody::Expr(x) => walk_expr_lints(x, out),
                    MatchArmBody::Block(b) => walk_block_lints(b, out),
                }
            }
        }
        ExprKind::For { iter, body, .. } | ExprKind::ParallelFor { iter, body, .. } => {
            walk_expr_lints(iter, out); walk_block_lints(body, out);
        }
        ExprKind::While { cond, body } => {
            walk_expr_lints(cond, out); walk_block_lints(body, out);
        }
        ExprKind::WhileLet { scrutinee, body, .. } => {
            walk_expr_lints(scrutinee, out); walk_block_lints(body, out);
        }
        ExprKind::Loop { body } => walk_block_lints(body, out),
        ExprKind::Block(b) => walk_block_lints(b, out),
        ExprKind::Spawn(x) => walk_expr_lints(x, out),
        ExprKind::Detach(b) => walk_block_lints(b, out),
        ExprKind::Supervised { body, cancel } => {
            walk_block_lints(body, out);
            if let Some(c) = cancel { walk_expr_lints(c, out); }
        }
        ExprKind::Forbid { body, .. } | ExprKind::Realtime { body, .. } => {
            walk_block_lints(body, out);
        }
        ExprKind::Throw(x) => walk_expr_lints(x, out),
        ExprKind::Interrupt(opt) => {
            if let Some(x) = opt { walk_expr_lints(x, out); }
        }
        ExprKind::Range { start, end, .. } => {
            walk_expr_lints(start, out); walk_expr_lints(end, out);
        }
        ExprKind::InterpolatedStr { parts } => {
            for p in parts {
                if let crate::ast::InterpStrPart::Expr(x) = p { walk_expr_lints(x, out); }
            }
        }
        ExprKind::TaggedTemplate { args, .. } => {
            for x in args { walk_expr_lints(x, out); }
        }
        ExprKind::Lambda { body, .. } => walk_expr_lints(body, out),
        ExprKind::ClosureLight { body, .. } => match body {
            ClosureBody::Expr(x) => walk_expr_lints(x, out),
            ClosureBody::Block(b) => walk_block_lints(b, out),
        },
        ExprKind::ClosureFull(sb) => match &sb.body {
            FnBody::Expr(x) => walk_expr_lints(x, out),
            FnBody::Block(b) => walk_block_lints(b, out),
            FnBody::External => {}
        },
        ExprKind::With { bindings, body } => {
            for b in bindings { walk_expr_lints(&b.handler, out); }
            walk_block_lints(body, out);
        }
        ExprKind::HandlerLit { methods, .. } => {
            for m in methods {
                match &m.body {
                    HandlerMethodBody::Expr(x) => walk_expr_lints(x, out),
                    HandlerMethodBody::Block(b) => walk_block_lints(b, out),
                }
            }
        }
        ExprKind::Select { arms } => {
            for arm in arms {
                match &arm.op {
                    crate::ast::SelectOp::Recv { chan, .. } => walk_expr_lints(chan, out),
                    crate::ast::SelectOp::Send { chan, value } => {
                        walk_expr_lints(chan, out); walk_expr_lints(value, out);
                    }
                    crate::ast::SelectOp::Default => {}
                }
                if let Some(g) = &arm.guard { walk_expr_lints(g, out); }
                walk_block_lints(&arm.body, out);
            }
        }
        // Листовые — нет под-выражений.
        ExprKind::Ident(_) | ExprKind::Path(_) | ExprKind::SelfAccess
        | ExprKind::IntLit(_) | ExprKind::FloatLit(_) | ExprKind::BoolLit(_)
        | ExprKind::StrLit(_) | ExprKind::CharLit(_) | ExprKind::UnitLit => {}
    }
}

/// Plan 52 Ф.2 (D108): lint-проверки map-литерала `[k: v]`.
///
/// - **duplicate-map-key**: два ключа — одинаковые compile-time константы
///   (int/str/bool literal). Last-wins семантика, но второй entry молча
///   затирает первый — паритет с `go vet` / `tsc`. Произвольные выражения
///   (`a`, `a+1`, `f()`) не проверяются.
/// - **nan-map-key**: ключ — константа `f64.NAN` / `f32.NAN`. По IEEE 754
///   `NaN != NaN`, поэтому вставленный ключ невозможно найти обратно.
fn check_map_literal_lints(pairs: &[(Expr, Expr)], out: &mut Vec<LintWarning>) {
    // NaN-key: ключ это Path(["f64", "NAN"]) или Path(["f32", "NAN"]).
    for (k, _) in pairs {
        if let ExprKind::Path(parts) = &k.kind {
            if parts.len() == 2
                && (parts[0] == "f64" || parts[0] == "f32")
                && parts[1] == "NAN"
            {
                out.push(LintWarning {
                    rule: "nan-map-key",
                    diag: Diagnostic::new(
                        format!(
                            "warning: `{}.NAN` as map key — inserted key can never be \
                             found (IEEE 754: NaN != NaN). Consider a sentinel value or \
                             a non-float key type.",
                            parts[0]
                        ),
                        k.span,
                    ),
                });
            }
        }
    }
    // duplicate-map-key: сравниваем константные ключи попарно. Канонизируем
    // в строковый дескриптор; non-const ключи дают None и не сравниваются.
    let consts: Vec<(Option<String>, Span)> = pairs
        .iter()
        .map(|(k, _)| (const_key_descriptor(k), k.span))
        .collect();
    for i in 0..consts.len() {
        let (Some(desc_i), _) = (&consts[i].0, consts[i].1) else { continue };
        for j in (i + 1)..consts.len() {
            let (Some(desc_j), span_j) = (&consts[j].0, consts[j].1) else { continue };
            if desc_i == desc_j {
                out.push(LintWarning {
                    rule: "duplicate-map-key",
                    diag: Diagnostic::new(
                        format!(
                            "warning: duplicate key `{}` in map literal — the later \
                             entry overwrites the earlier one (last-wins)",
                            human_key(&consts[j].0, pairs, j)
                        ),
                        span_j,
                    ),
                });
                break; // один warning на дубликат — не плодим N²
            }
        }
    }
}

/// Канонический дескриптор compile-time-константного ключа для сравнения
/// дубликатов. `None` — ключ не является распознаваемой константой.
/// Дескриптор включает префикс типа, чтобы `1` (int) и `"1"` (str) не
/// считались дубликатами.
fn const_key_descriptor(k: &Expr) -> Option<String> {
    match &k.kind {
        ExprKind::IntLit(n) => Some(format!("int:{n}")),
        ExprKind::StrLit(s) => Some(format!("str:{s}")),
        ExprKind::BoolLit(b) => Some(format!("bool:{b}")),
        ExprKind::CharLit(c) => Some(format!("char:{c}")),
        // Унарный минус над int-литералом — `-1` как ключ.
        ExprKind::Unary { op: crate::ast::UnOp::Neg, operand } => {
            if let ExprKind::IntLit(n) = &operand.kind {
                Some(format!("int:{}", -n))
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Человекочитаемое представление ключа для текста warning'а.
fn human_key(desc: &Option<String>, pairs: &[(Expr, Expr)], idx: usize) -> String {
    match &pairs[idx].0.kind {
        ExprKind::IntLit(n) => n.to_string(),
        ExprKind::StrLit(s) => format!("\"{s}\""),
        ExprKind::BoolLit(b) => b.to_string(),
        ExprKind::CharLit(c) => {
            char::from_u32(*c).map(|ch| format!("'{ch}'")).unwrap_or_else(|| format!("'\\u{{{c:x}}}'"))
        }
        _ => desc.clone().unwrap_or_else(|| "<key>".to_string()),
    }
}

/// Собирает имена user-defined эффектов: `type X effect { ... }`.
/// Также включает встроенные stdlib effects из prelude (D26 + D62).
fn collect_effect_names(m: &Module) -> HashSet<String> {
    let mut names: HashSet<String> = [
        "Fail", "Io", "Net", "Db", "Fs", "Time", "Random",
        "Log", "Trace", "Ask", "Alloc", "Detach", "Blocking", "Mem",
    ].iter().map(|s| s.to_string()).collect();
    for item in &m.items {
        if let Item::Type(td) = item {
            if matches!(td.kind, TypeDeclKind::Effect(_)) {
                names.insert(td.name.clone());
            }
        }
    }
    names
}

/// Собирает имена user-defined protocols: `type X protocol { ... }`.
/// Также включает встроенные prelude protocols.
///
/// Plan 15 D53 strict: после split'а `TypeDeclKind::Protocol(_)` —
/// scan'имся по нему напрямую (раньше было закомменчено потому что
/// все protocols/effects попадали в Effect-variant).
fn collect_protocol_names(m: &Module) -> HashSet<String> {
    let mut names: HashSet<String> = [
        "Hashable", "Ord", "Eq", "Iter", "From", "Into",
        "TryFrom", "TryInto", "ToStr",
    ].iter().map(|s| s.to_string()).collect();
    for item in &m.items {
        if let Item::Type(td) = item {
            if matches!(td.kind, TypeDeclKind::Protocol(_)) {
                names.insert(td.name.clone());
            }
        }
    }
    names
}

/// Rule: `protocol-in-effect-position` — `fn f() Hashable -> ()` где
/// `Hashable` это protocol. Should be `fn f(x T Hashable) -> ()` (как
/// generic-bound на параметре, D72) или `fn f[T Hashable](x T) -> ()`.
fn check_protocol_in_effect_position(
    f: &FnDecl,
    protocols: &HashSet<String>,
    effects: &HashSet<String>,
    out: &mut Vec<LintWarning>,
) {
    for eff in &f.effects {
        if let TypeRef::Named { path, .. } = eff {
            if path.len() == 1 {
                let name = &path[0];
                if protocols.contains(name) && !effects.contains(name) {
                    out.push(LintWarning {
                        rule: "protocol-in-effect-position",
                        diag: Diagnostic::new(
                            format!(
                                "warning: `{}` is a protocol, not an effect, but appears in \
                                 effect-position (between `)` and `->`) of fn `{}` \
                                 (D62: protocols are structural type-bounds, not handler-substitutable; \
                                 use `fn {} (x T {}) -> ...` or generic-bound `[T {}]` instead)",
                                name, f.name, f.name, name, name
                            ),
                            eff.span(),
                        ),
                    });
                }
            }
        }
    }
}

fn check_fn(f: &FnDecl, out: &mut Vec<LintWarning>) {
    if !f.is_export {
        return;
    }
    // Rule: export-fail-untyped — `Fail` без [E] в public API.
    for eff in &f.effects {
        if is_fail_untyped(eff) {
            let span = eff.span();
            out.push(LintWarning {
                rule: "export-fail-untyped",
                diag: Diagnostic::new(
                    format!(
                        "warning: export fn `{}` uses `Fail` without type parameter \
                         (D65 convention: public API should specify `Fail[E]` with concrete error type; \
                         use `Fail[any]` to opt into explicit erasure)",
                        f.name
                    ),
                    span,
                ),
            });
        }
    }
}

/// `Fail` без generic-параметра. Не путаем с `Fail[E]` (typed) или
/// `Fail[any]` (явная erasure — программист сознательно opt-in).
fn is_fail_untyped(ty: &TypeRef) -> bool {
    if let TypeRef::Named { path, generics, .. } = ty {
        if path.len() == 1 && path[0] == "Fail" && generics.is_empty() {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::lex;
    use crate::parser::Parser;

    fn parse(src: &str) -> Module {
        let toks = lex(src).unwrap();
        let mut p = Parser::new(toks);
        p.parse_module().unwrap()
    }

    #[test]
    fn warns_on_export_fail_untyped() {
        let m = parse("module foo\nexport fn parse(s str) Fail -> int => 0\n");
        let ws = lint_module(&m);
        assert_eq!(ws.len(), 1);
        assert_eq!(ws[0].rule, "export-fail-untyped");
    }

    #[test]
    fn no_warning_on_export_fail_typed() {
        let m = parse("module foo\nexport fn parse(s str) Fail[ParseError] -> int => 0\n");
        let ws = lint_module(&m);
        assert_eq!(ws.len(), 0);
    }

    #[test]
    fn no_warning_on_export_fail_any() {
        // Fail[any] — explicit erasure, программист opt-in
        let m = parse("module foo\nexport fn dump() Fail[any] -> () => ()\n");
        let ws = lint_module(&m);
        assert_eq!(ws.len(), 0);
    }

    #[test]
    fn no_warning_on_private_fail() {
        // Private fn — Fail без E это inference placeholder, OK
        let m = parse("module foo\nfn parse(s str) Fail -> int => 0\n");
        let ws = lint_module(&m);
        assert_eq!(ws.len(), 0);
    }

    #[test]
    fn warns_on_protocol_in_effect_position() {
        // Hashable — встроенный protocol; в effect-position warning.
        let m = parse("module foo\nfn process(x int) Hashable -> int => x\n");
        let ws = lint_module(&m);
        assert_eq!(ws.len(), 1);
        assert_eq!(ws[0].rule, "protocol-in-effect-position");
    }

    #[test]
    fn no_warning_on_effect_in_effect_position() {
        // Db — effect, OK в effect-position.
        let m = parse("module foo\nfn lookup(id int) Db -> int => id\n");
        let ws = lint_module(&m);
        assert_eq!(ws.len(), 0);
    }
}
