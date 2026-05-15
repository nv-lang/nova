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

use std::collections::HashMap;

use crate::ast::{Expr, ExprKind, BinOp, UnOp};
use super::ir::*;

#[derive(Debug, Clone)]
pub enum EncodingError {
    /// Конструкция не поддерживается trivial-encoder'ом 33.1.
    Unsupported(String),
}

/// Plan 33.3 Ф.9: контекст encoder'а — реестр pure_view-ops модуля.
/// Ключ — pure_view name (e.g. "balance"), значение — (effect_name,
/// return_sort). Используется для конвертации `balance(id)` →
/// uninterpreted function `_view_Db_balance(id)` SMT-stide.
#[derive(Debug, Clone)]
pub struct EncodeCtx<'a> {
    pub pure_views: &'a HashMap<String, PureViewSig>,
    /// Plan 33.4 D.0.2: реестр `#pure` fn-ов модуля. Ключ — fn name.
    /// Используется для кодирования `is_positive(n)` в контракте →
    /// UF `_pure_fn_is_positive(n)` в SMT.
    pub pure_fns: &'a HashMap<String, PureFnInfo>,
}

/// Signature of a `#pure` fn for SMT encoding (Plan 33.4 D.0.2).
#[derive(Debug, Clone)]
pub struct PureFnInfo {
    pub param_names: Vec<String>,
    pub param_sorts: Vec<SortRef>,
    pub return_sort: SortRef,
    /// Body expression for inlining. If present, calls to this fn in contracts
    /// are inlined (substituting args for params) rather than encoded as UF.
    /// This gives Z3 immediate ground truth without quantifier instantiation.
    pub body_expr: Option<Box<Expr>>,
}

/// SMT UF name for a pure fn: `_pure_fn_<name>`.
pub fn pure_fn_uf_name(fn_name: &str) -> String {
    format!("_pure_fn_{}", fn_name)
}

#[derive(Debug, Clone)]
pub struct PureViewSig {
    pub effect_name: String,
    pub arity: usize,
    /// Sort возвращаемого значения. Используется backend'ом для
    /// типизированной декларации UF (Z3 нужно знать range sort).
    pub return_sort: SortRef,
    /// Sorts параметров (тоже для UF declaration).
    pub param_sorts: Vec<SortRef>,
}

impl<'a> EncodeCtx<'a> {
    /// Empty context — pure_view-ы не известны (старые тесты + bootstrap).
    pub fn empty() -> EncodeCtx<'static> {
        // Хитрый трюк: возвращаем 'static reference на пустую map.
        // Используется только для backward-compat encode_expr.
        static EMPTY_VIEWS: std::sync::OnceLock<HashMap<String, PureViewSig>> = std::sync::OnceLock::new();
        static EMPTY_FNS: std::sync::OnceLock<HashMap<String, PureFnInfo>> = std::sync::OnceLock::new();
        let views = EMPTY_VIEWS.get_or_init(HashMap::new);
        let fns = EMPTY_FNS.get_or_init(HashMap::new);
        EncodeCtx { pure_views: views, pure_fns: fns }
    }
}

/// Helper для UF имени pure_view: `_view_<EffectName>_<OpName>`.
pub fn pure_view_uf_name(effect: &str, op: &str) -> String {
    format!("_view_{}_{}", effect, op)
}

/// Encode Nova-expr в SMT-term (без context'а — backward-compat).
pub fn encode_expr(e: &Expr) -> Result<SmtTerm, EncodingError> {
    let ctx = EncodeCtx::empty();
    encode_expr_with_ctx(e, &ctx)
}

/// Encode Nova-expr в SMT-term с контекстом pure_view'ов.
pub fn encode_expr_with_ctx(e: &Expr, ctx: &EncodeCtx) -> Result<SmtTerm, EncodingError> {
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
                        let inner = encode_expr_with_ctx(args[0].expr(), ctx)?;
                        // Name based on pretty-print для стабильности.
                        let key = format!("_old_{}", sanitize_for_var(&inner.pretty()));
                        return Ok(SmtTerm::Var(key));
                    }
                }
            }
            // Plan 33.3 Ф.9: pure_view-call → UF `_view_<Effect>_<Op>`.
            // Type-check уже проверил что эффект в сигнатуре fn (Ф.9.3
            // part 2), здесь — просто конвертация в SMT-IR.
            if trailing.is_none() {
                if let ExprKind::Ident(name) = &func.kind {
                    if let Some(sig) = ctx.pure_views.get(name) {
                        if args.len() != sig.arity {
                            return Err(EncodingError::Unsupported(format!(
                                "pure_view `{}.{}` arity mismatch: expected {}, got {}",
                                sig.effect_name, name, sig.arity, args.len(),
                            )));
                        }
                        let mut encoded_args = Vec::with_capacity(args.len());
                        for a in args {
                            encoded_args.push(encode_expr_with_ctx(a.expr(), ctx)?);
                        }
                        let uf = pure_view_uf_name(&sig.effect_name, name);
                        return Ok(SmtTerm::App(uf, encoded_args));
                    }
                }
            }
            // Plan 33.4 D.0.2: #pure fn composition.
            // If body is available, inline (substitute args for params) to give
            // Z3 ground truth without quantifier instantiation. Otherwise fall
            // back to UF application (+ forall axiom in pipeline).
            if trailing.is_none() {
                if let ExprKind::Ident(name) = &func.kind {
                    if let Some(info) = ctx.pure_fns.get(name) {
                        if args.len() != info.param_sorts.len() {
                            return Err(EncodingError::Unsupported(format!(
                                "pure fn `{}` arity mismatch: expected {}, got {}",
                                name, info.param_sorts.len(), args.len()
                            )));
                        }
                        if let Some(body_e) = &info.body_expr {
                            // Inline: encode body with params substituted by encoded args.
                            let mut term = encode_expr_with_ctx(body_e, ctx)?;
                            for (param_name, call_arg) in info.param_names.iter().zip(args.iter()) {
                                let enc_arg = encode_expr_with_ctx(call_arg.expr(), ctx)?;
                                term = term.substitute(param_name, &enc_arg);
                            }
                            return Ok(term);
                        } else {
                            // No body → UF application.
                            let mut encoded_args = Vec::with_capacity(args.len());
                            for a in args {
                                encoded_args.push(encode_expr_with_ctx(a.expr(), ctx)?);
                            }
                            let uf = pure_fn_uf_name(name);
                            return Ok(SmtTerm::App(uf, encoded_args));
                        }
                    }
                }
            }
            Err(EncodingError::Unsupported(format!(
                "call expressions in contracts not yet supported in Plan 33.1 \
                 (Plan 33.2 composition with `#pure` functions)"
            )))
        }

        ExprKind::Binary { op, left, right } => {
            let l = encode_expr_with_ctx(left, ctx)?;
            let r = encode_expr_with_ctx(right, ctx)?;
            let op_str = bin_op_to_smt(*op)?;
            Ok(SmtTerm::App(op_str.into(), vec![l, r]))
        }

        ExprKind::Unary { op, operand } => {
            let v = encode_expr_with_ctx(operand, ctx)?;
            match op {
                UnOp::Not => Ok(SmtTerm::App("not".into(), vec![v])),
                UnOp::Neg => Ok(SmtTerm::App("-".into(),
                    vec![SmtTerm::IntLit(0), v])),
            }
        }

        // `if cond { then } else { else_ }` → ite(cond, then, else)
        // через `(or (and cond then) (and (not cond) else))`.
        ExprKind::If { cond, then, else_ } => {
            let cond_term = encode_expr_with_ctx(cond, ctx)?;
            // Block must be single expression for trivial encoding.
            if !then.stmts.is_empty() { return Err(EncodingError::Unsupported(
                "if-branch with statements not supported in trivial encoder".into())); }
            let then_term = match &then.trailing {
                Some(e) => encode_expr_with_ctx(e, ctx)?,
                None => return Err(EncodingError::Unsupported(
                    "if without trailing expression".into())),
            };
            let else_term = match else_ {
                Some(crate::ast::ElseBranch::Block(b)) => {
                    if !b.stmts.is_empty() { return Err(EncodingError::Unsupported(
                        "else-branch with statements not supported".into())); }
                    match &b.trailing {
                        Some(e) => encode_expr_with_ctx(e, ctx)?,
                        None => return Err(EncodingError::Unsupported(
                            "else-block without trailing".into())),
                    }
                }
                Some(crate::ast::ElseBranch::If(e)) => encode_expr_with_ctx(e, ctx)?,
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
            let obj_t = encode_expr_with_ctx(obj, ctx)?;
            Ok(SmtTerm::App(format!("_field_{}", name), vec![obj_t]))
        }

        // D.1.3: forall x in lo..hi : P(x)
        ExprKind::Forall { var, range, body } => {
            let (lo, hi) = extract_range(range)?;
            let lo_t = encode_expr_with_ctx(lo, ctx)?;
            let hi_t = encode_expr_with_ctx(hi, ctx)?;
            let body_t = encode_expr_with_ctx(body, ctx)?;
            let var_s = SmtTerm::Var(var.clone());
            // range constraint: lo <= x && x < hi
            let in_range = SmtTerm::and(vec![
                SmtTerm::App("<=".into(), vec![lo_t, var_s.clone()]),
                SmtTerm::App("<".into(), vec![var_s, hi_t]),
            ]);
            // forall x: Int. in_range => body
            let implies = SmtTerm::App("=>".into(), vec![in_range, body_t]);
            // Ф.1.2 (Plan 33.5): extract trigger patterns from body.
            // Ищем App(uf, args) содержащий bound_var → передаём как trigger
            // в SmtTerm::Forall.patterns, Z3 backend использует Z3_mk_pattern.
            let patterns = collect_triggers(&implies, var);
            Ok(SmtTerm::Forall(vec![(var.clone(), SortRef::Int)], patterns, Box::new(implies)))
        }

        // D.1.3: exists x in lo..hi : P(x)
        // Кодируем как not(forall x in range: not P(x))
        ExprKind::Exists { var, range, body } => {
            let (lo, hi) = extract_range(range)?;
            let lo_t = encode_expr_with_ctx(lo, ctx)?;
            let hi_t = encode_expr_with_ctx(hi, ctx)?;
            let body_t = encode_expr_with_ctx(body, ctx)?;
            let var_s = SmtTerm::Var(var.clone());
            let in_range = SmtTerm::and(vec![
                SmtTerm::App("<=".into(), vec![lo_t, var_s.clone()]),
                SmtTerm::App("<".into(), vec![var_s, hi_t]),
            ]);
            let not_body = SmtTerm::not(body_t);
            let implies = SmtTerm::App("=>".into(), vec![in_range, not_body]);
            // Ф.1.2: triggers для двойного-отрицания exists (по body, не not_body).
            // Ищем в implies (который содержит и not_body).
            let patterns = collect_triggers(&implies, var);
            let inner = SmtTerm::Forall(vec![(var.clone(), SortRef::Int)], patterns, Box::new(implies));
            Ok(SmtTerm::not(inner))
        }

        _ => Err(EncodingError::Unsupported(format!(
            "expression kind not supported in contract encoder (33.1)"
        ))),
    }
}

/// Ф.1.2 (Plan 33.5): собирает trigger patterns для квантора над `bound_var`.
///
/// Алгоритм:
/// 1. Обходим body рекурсивно, собираем **все** App(name, args) где:
///    - name не является логическим оператором (=>, and, or, not, =, !=, <, <=, >, >=).
///    - хотя бы один arg содержит `bound_var` (прямо или косвенно).
/// 2. Возвращаем их как `Vec<Vec<SmtTerm>>` — один pattern per найденный App.
///    Z3 попробует каждый pattern независимо; достаточно матча одного.
/// 3. Если triggers не найдены — возвращаем пустой вектор (no-hint,
///    Z3 использует heuristic instantiation).
///
/// Паритет с Dafny: Dafny автоматически выводит triggers; Verus требует
/// явных `#[trigger]`. Nova автовыводит как Dafny, но без SAT fallback.
pub fn collect_triggers(body: &SmtTerm, bound_var: &str) -> Vec<Vec<SmtTerm>> {
    let mut found: Vec<SmtTerm> = Vec::new();
    collect_trigger_apps(body, bound_var, &mut found);
    if found.is_empty() {
        vec![]
    } else {
        // Каждый найденный App — отдельный single-term pattern.
        found.into_iter().map(|t| vec![t]).collect()
    }
}

/// Рекурсивный walker для collect_triggers.
fn collect_trigger_apps(t: &SmtTerm, bound_var: &str, out: &mut Vec<SmtTerm>) {
    match t {
        SmtTerm::App(name, args) => {
            let is_logic_op = matches!(name.as_str(),
                "=>" | "and" | "or" | "not" | "=" | "!=" | "<" | "<=" | ">" | ">=" | "ite");
            if !is_logic_op && args.iter().any(|a| term_contains_var(a, bound_var)) {
                // Хороший trigger: UF или arithmetic fn содержащий bound var.
                out.push(t.clone());
                // Не рекурсируем в args — inner triggers less precise.
                return;
            }
            // Для логических операторов рекурсируем в аргументы.
            for a in args {
                collect_trigger_apps(a, bound_var, out);
            }
        }
        SmtTerm::Forall(_, _, inner) => collect_trigger_apps(inner, bound_var, out),
        _ => {}
    }
}

/// Ф.1.2: проверяет, содержит ли term переменную `var_name`.
pub fn term_contains_var(t: &SmtTerm, var_name: &str) -> bool {
    match t {
        SmtTerm::Var(n) => n == var_name,
        SmtTerm::App(_, args) => args.iter().any(|a| term_contains_var(a, var_name)),
        SmtTerm::Forall(_, _, body) => term_contains_var(body, var_name),
        _ => false,
    }
}

/// D.1.3: извлечь lo и hi из Range-выражения.
fn extract_range(e: &Expr) -> Result<(&Expr, &Expr), EncodingError> {
    match &e.kind {
        ExprKind::Range { start, end, .. } => Ok((start, end)),
        _ => Err(EncodingError::Unsupported(
            "quantifier range must be lo..hi expression".into())),
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
