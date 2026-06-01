//! Plan 114.4.2 (D199) Ф.2 — comptime evaluator subsystem.
//!
//! Evaluates `const fn` calls at compile time. Pure-functional
//! environment-based interpreter over Nova AST (no IR lowering).
//! Operates on the V1 subset enforced by `check_const_fn_*` в
//! `types/mod.rs`: literals + arithmetic + as-casts + ident refs to
//! const params/locals + local const decls + final expression + direct
//! calls to other const fn.
//!
//! Memoization per compilation: `(fn_name, arg-tuple) → ConstValue`.
//!
//! Errors surfaced via `Diagnostic` для caller'а:
//! - `E_CONST_FN_EVAL_OVERFLOW` — checked-arithmetic overflow
//!   (signed i64 wraparound treated as overflow в const context, not
//!   silent 2's complement).
//! - `E_CONST_FN_DIV_ZERO` — division/modulo by zero.
//! - `E_CONST_FN_EVAL_PANIC` — defensive catch-all (should not fire
//!   given checker; if fires — bug в evaluator / checker drift).
//!
//! Architecture: single shared `ConstFnEvaluator` instance per module
//! lives in `desugar` pass. AST rewriter walks every `Expr::Call` где
//! callee resolves to a const fn name and replaces the entire `Call`
//! node с literal `Expr` of the evaluated value (типа IntLit / StrLit
//! / etc.). Dead const fn (no call sites) emit nothing — codegen drop
//! handled separately by `emit_c` skip rule.

use std::collections::HashMap;

use crate::ast::{Block, Expr, ExprKind, FnBody, FnDecl, Stmt, Pattern};
use crate::diag::{Diagnostic, Span};

/// Comptime value space — strict subset of Nova primitive values that
/// can be produced by a V1 const fn body. No heap-allocated containers
/// (arrays/maps/records/tuples) — checker rejects allocations.
#[derive(Debug, Clone)]
pub enum ConstValue {
    Int(i64),
    Float(f64),
    Bool(bool),
    /// Strings are immutable literals; cloned on bind/return.
    Str(String),
    /// Char codepoint (Plan Q-char-literals: u32).
    Char(u32),
    Unit,
}

// Manual Eq/Hash: f64 not Hash, comparing by bit-pattern (NaN-NaN
// strict bitwise compare → distinct в memo cache lookup, conservative
// correct).
impl PartialEq for ConstValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (ConstValue::Int(a), ConstValue::Int(b)) => a == b,
            (ConstValue::Float(a), ConstValue::Float(b)) => a.to_bits() == b.to_bits(),
            (ConstValue::Bool(a), ConstValue::Bool(b)) => a == b,
            (ConstValue::Str(a), ConstValue::Str(b)) => a == b,
            (ConstValue::Char(a), ConstValue::Char(b)) => a == b,
            (ConstValue::Unit, ConstValue::Unit) => true,
            _ => false,
        }
    }
}
impl Eq for ConstValue {}
impl std::hash::Hash for ConstValue {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        match self {
            ConstValue::Int(v) => { 0u8.hash(state); v.hash(state); }
            ConstValue::Float(v) => { 1u8.hash(state); v.to_bits().hash(state); }
            ConstValue::Bool(v) => { 2u8.hash(state); v.hash(state); }
            ConstValue::Str(v) => { 3u8.hash(state); v.hash(state); }
            ConstValue::Char(v) => { 4u8.hash(state); v.hash(state); }
            ConstValue::Unit => { 5u8.hash(state); }
        }
    }
}

impl ConstValue {
    /// Convert evaluated value back to AST literal expression (for
    /// AST-rewrite at call sites). Span — span исходного Call expr.
    pub fn to_literal_expr(&self, span: Span) -> Expr {
        let kind = match self {
            ConstValue::Int(v) => ExprKind::IntLit(*v),
            ConstValue::Float(v) => ExprKind::FloatLit(*v),
            ConstValue::Bool(v) => ExprKind::BoolLit(*v),
            ConstValue::Str(v) => ExprKind::StrLit(v.clone()),
            ConstValue::Char(v) => ExprKind::CharLit(*v),
            ConstValue::Unit => ExprKind::UnitLit,
        };
        Expr { kind, span }
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            ConstValue::Int(_) => "int",
            ConstValue::Float(_) => "f64",
            ConstValue::Bool(_) => "bool",
            ConstValue::Str(_) => "str",
            ConstValue::Char(_) => "char",
            ConstValue::Unit => "Unit",
        }
    }
}

/// Recursion depth budget (defensive — checker rejects recursion, but
/// in case mutual cycle escapes detection, this stops the evaluator
/// from blowing the stack).
const MAX_EVAL_DEPTH: usize = 64;

pub struct ConstFnEvaluator<'a> {
    /// Const fn declarations indexed by name. Owner is module.items via
    /// caller — we hold a reference.
    fns: HashMap<String, &'a FnDecl>,
    /// Memoization cache: `(fn_name, args)` → result.
    /// Args are compared by ConstValue PartialEq; floats compare bitwise
    /// (NaN-NaN considered distinct only if bits differ — V1 const fn
    /// doesn't produce NaN сейчас, but defensive).
    memo: HashMap<(String, Vec<ConstValue>), ConstValue>,
}

impl<'a> ConstFnEvaluator<'a> {
    pub fn new(items: &'a [crate::ast::Item]) -> Self {
        let mut fns = HashMap::new();
        for item in items {
            if let crate::ast::Item::Fn(fd) = item {
                let is_const = fd.return_is_const || fd.params.iter().any(|p| p.is_const);
                if is_const {
                    fns.insert(fd.name.clone(), fd);
                }
            }
        }
        Self { fns, memo: HashMap::new() }
    }

    /// Returns true if `name` is a registered const fn.
    pub fn is_const_fn(&self, name: &str) -> bool {
        self.fns.contains_key(name)
    }

    pub fn const_fn_names(&self) -> impl Iterator<Item = &String> {
        self.fns.keys()
    }

    /// Top-level entry: evaluate `f(args)` где f — const fn.
    /// `call_span` — span исходного call site (для error reporting).
    pub fn eval_call(
        &mut self,
        name: &str,
        args: Vec<ConstValue>,
        call_span: Span,
    ) -> Result<ConstValue, Diagnostic> {
        self.eval_call_inner(name, args, call_span, 0)
    }

    fn eval_call_inner(
        &mut self,
        name: &str,
        args: Vec<ConstValue>,
        call_span: Span,
        depth: usize,
    ) -> Result<ConstValue, Diagnostic> {
        if depth >= MAX_EVAL_DEPTH {
            return Err(Diagnostic::new(
                format!(
                    "[E_CONST_FN_EVAL_PANIC] const fn evaluation exceeded depth \
                     limit {} в call to `{}` — likely a recursion-detection bug \
                     (checker should have rejected this). Plan 114.4.2 D199.",
                    MAX_EVAL_DEPTH, name
                ),
                call_span,
            ));
        }
        let key = (name.to_string(), args.clone());
        if let Some(v) = self.memo.get(&key) {
            return Ok(v.clone());
        }
        let fd = self.fns.get(name).copied().ok_or_else(|| {
            Diagnostic::new(
                format!(
                    "[E_CONST_FN_EVAL_PANIC] const fn `{}` not found в registry — \
                     evaluator/checker drift. Plan 114.4.2 D199.",
                    name
                ),
                call_span,
            )
        })?;
        if fd.params.len() != args.len() {
            return Err(Diagnostic::new(
                format!(
                    "[E_CONST_FN_EVAL_PANIC] arity mismatch calling const fn `{}` — \
                     expected {}, got {} (call-site validation failed). D199.",
                    name, fd.params.len(), args.len()
                ),
                call_span,
            ));
        }
        // Build param env: param_name → ConstValue.
        let mut env: HashMap<String, ConstValue> = HashMap::new();
        for (p, v) in fd.params.iter().zip(args.iter()) {
            env.insert(p.name.clone(), v.clone());
        }
        // Type-check each param value against declared type (best-effort
        // for V1: int/i64/f64/str/bool/char/Unit names).
        for (p, v) in fd.params.iter().zip(args.iter()) {
            check_value_matches_type(v, &p.ty, call_span)?;
        }
        let result = match &fd.body {
            FnBody::Expr(e) => self.eval_expr(e, &env, name, depth + 1)?,
            FnBody::Block(b) => self.eval_block(b, &env, name, depth + 1)?,
            FnBody::External => {
                return Err(Diagnostic::new(
                    format!(
                        "[E_CONST_FN_EXTERNAL] external const fn `{}` cannot be \
                         comptime-evaluated (D199).",
                        name
                    ),
                    fd.span,
                ));
            }
        };
        self.memo.insert(key, result.clone());
        Ok(result)
    }

    fn eval_block(
        &mut self,
        block: &Block,
        env: &HashMap<String, ConstValue>,
        current_fn: &str,
        depth: usize,
    ) -> Result<ConstValue, Diagnostic> {
        let mut env = env.clone();
        for st in &block.stmts {
            match st {
                Stmt::Const(cd) => {
                    let v = self.eval_expr(&cd.value, &env, current_fn, depth)?;
                    env.insert(cd.name.clone(), v);
                }
                Stmt::Expr(e) => {
                    // Last stmt в block (if no trailing) — final expression.
                    // Intermediate Stmt::Expr rejected by checker, so if it
                    // appears here it must be the last.
                    return self.eval_expr(e, &env, current_fn, depth);
                }
                Stmt::Return { value: Some(e), .. } => {
                    return self.eval_expr(e, &env, current_fn, depth);
                }
                Stmt::Return { value: None, span } => {
                    // const fn with -> const T must produce a value — bare
                    // return with no value is invalid for V1.
                    return Err(Diagnostic::new(
                        "[E_CONST_FN_EVAL_PANIC] bare `return` без значения \
                         в const fn — V1 requires explicit return value (D199)."
                            .to_string(),
                        *span,
                    ));
                }
                _ => {
                    return Err(Diagnostic::new(
                        "[E_CONST_FN_EVAL_PANIC] unexpected stmt в const fn \
                         body during evaluation — checker should reject (D199)."
                            .to_string(),
                        block.span,
                    ));
                }
            }
        }
        // Trailing expression — final value of block.
        if let Some(trail) = &block.trailing {
            return self.eval_expr(trail, &env, current_fn, depth);
        }
        // Empty block / unit body — V1 const fn must produce a value.
        Err(Diagnostic::new(
            "[E_CONST_FN_EVAL_PANIC] const fn body produced no value — V1 \
             requires final expression or `return expr` (D199)."
                .to_string(),
            block.span,
        ))
    }

    fn eval_expr(
        &mut self,
        expr: &Expr,
        env: &HashMap<String, ConstValue>,
        current_fn: &str,
        depth: usize,
    ) -> Result<ConstValue, Diagnostic> {
        use crate::ast::{BinOp, UnOp};
        match &expr.kind {
            ExprKind::IntLit(v) => Ok(ConstValue::Int(*v)),
            ExprKind::FloatLit(v) => Ok(ConstValue::Float(*v)),
            ExprKind::StrLit(v) => Ok(ConstValue::Str(v.clone())),
            ExprKind::BoolLit(v) => Ok(ConstValue::Bool(*v)),
            ExprKind::CharLit(v) => Ok(ConstValue::Char(*v)),
            ExprKind::UnitLit => Ok(ConstValue::Unit),
            ExprKind::Ident(name) => env
                .get(name)
                .cloned()
                .ok_or_else(|| Diagnostic::new(
                    format!(
                        "[E_CONST_FN_EVAL_PANIC] unbound identifier `{}` в const \
                         fn body — checker drift (D199).",
                        name
                    ),
                    expr.span,
                )),
            ExprKind::Unary { op, operand } => {
                let v = self.eval_expr(operand, env, current_fn, depth)?;
                eval_unary(*op, &v, expr.span)
            }
            ExprKind::Binary { op, left, right } => {
                let l = self.eval_expr(left, env, current_fn, depth)?;
                let r = self.eval_expr(right, env, current_fn, depth)?;
                eval_binary(*op, &l, &r, expr.span)
            }
            ExprKind::As(inner, target) => {
                let v = self.eval_expr(inner, env, current_fn, depth)?;
                eval_as_cast(&v, target, expr.span)
            }
            ExprKind::Call { func, args, trailing: None } => {
                // Callee must be Ident (checker enforces).
                let callee_name = if let ExprKind::Ident(n) = &func.kind {
                    n.clone()
                } else {
                    return Err(Diagnostic::new(
                        "[E_CONST_FN_EVAL_PANIC] non-ident callee в const fn \
                         body — checker drift (D199).".to_string(),
                        expr.span,
                    ));
                };
                // Evaluate args (positional only per checker).
                let mut arg_vals = Vec::with_capacity(args.len());
                for a in args {
                    if let crate::ast::CallArg::Item(e) = a {
                        arg_vals.push(self.eval_expr(e, env, current_fn, depth)?);
                    } else {
                        return Err(Diagnostic::new(
                            "[E_CONST_FN_EVAL_PANIC] non-positional arg в const \
                             fn body — checker drift (D199).".to_string(),
                            expr.span,
                        ));
                    }
                }
                self.eval_call_inner(&callee_name, arg_vals, expr.span, depth)
            }
            ExprKind::Block(b) => self.eval_block(b, env, current_fn, depth),
            // Plan 114.4.3 Ф.1 (V2): If — evaluate cond → bool; then/else branch.
            ExprKind::If { cond, then, else_ } => {
                let c = self.eval_expr(cond, env, current_fn, depth)?;
                let b = match c {
                    ConstValue::Bool(b) => b,
                    _ => return Err(Diagnostic::new(
                        format!(
                            "[E_CONST_FN_EVAL_PANIC] if-condition не bool (got {}) \
                             (D199).", c.type_name()
                        ),
                        cond.span,
                    )),
                };
                if b {
                    self.eval_block(then, env, current_fn, depth)
                } else {
                    match else_ {
                        Some(crate::ast::ElseBranch::Block(blk)) =>
                            self.eval_block(blk, env, current_fn, depth),
                        Some(crate::ast::ElseBranch::If(ie)) =>
                            self.eval_expr(ie, env, current_fn, depth),
                        None => Err(Diagnostic::new(
                            "[E_CONST_FN_EVAL_PANIC] if-без-else в const fn body — \
                             checker should reject (D199 V2).".to_string(),
                            expr.span,
                        )),
                    }
                }
            }
            // Plan 114.4.3 Ф.1 (V2): Match — evaluate scrutinee; find first
            // matching arm; evaluate its body. Pattern V2.0 subset:
            // literal / wildcard / ident-bind / Or-alternation.
            ExprKind::Match { scrutinee, arms } => {
                let sv = self.eval_expr(scrutinee, env, current_fn, depth)?;
                for arm in arms {
                    if let Some(bindings) = match_const_pattern(&arm.pattern, &sv) {
                        let mut new_env = env.clone();
                        for (k, v) in bindings { new_env.insert(k, v); }
                        if let Some(g) = &arm.guard {
                            let gv = self.eval_expr(g, &new_env, current_fn, depth)?;
                            if !matches!(gv, ConstValue::Bool(true)) { continue; }
                        }
                        return match &arm.body {
                            crate::ast::MatchArmBody::Expr(e) =>
                                self.eval_expr(e, &new_env, current_fn, depth),
                            crate::ast::MatchArmBody::Block(b) =>
                                self.eval_block(b, &new_env, current_fn, depth),
                        };
                    }
                }
                Err(Diagnostic::new(
                    format!(
                        "[E_CONST_FN_MATCH_EXHAUSTIVE] match не covered scrutinee \
                         value {:?} (D199 V2). Add wildcard `_` arm.",
                        sv
                    ),
                    expr.span,
                ))
            }
            _ => Err(Diagnostic::new(
                "[E_CONST_FN_EVAL_PANIC] expression form not supported by const \
                 fn evaluator — checker should reject (D199).".to_string(),
                expr.span,
            )),
        }
    }
}

/// Plan 114.4.3 Ф.1: pattern-match V2.0 subset.
/// Returns Some(bindings) if matches, None otherwise.
/// Bindings: Ident pattern → (name, scrutinee_value).
fn match_const_pattern(
    pat: &crate::ast::Pattern,
    val: &ConstValue,
) -> Option<Vec<(String, ConstValue)>> {
    use crate::ast::{Pattern, Literal};
    match pat {
        Pattern::Wildcard(_) => Some(Vec::new()),
        Pattern::Ident { name, is_mut: false, .. } => {
            Some(vec![(name.clone(), val.clone())])
        }
        Pattern::Literal(lit, _) => {
            let matches = match (lit, val) {
                (Literal::Int(a), ConstValue::Int(b)) => a == b,
                (Literal::Float(a), ConstValue::Float(b)) => a.to_bits() == b.to_bits(),
                (Literal::Str(a), ConstValue::Str(b)) => a == b,
                (Literal::Bool(a), ConstValue::Bool(b)) => a == b,
                (Literal::Char(a), ConstValue::Char(b)) => a == b,
                _ => false,
            };
            if matches { Some(Vec::new()) } else { None }
        }
        Pattern::Or { alternatives, .. } => {
            for alt in alternatives {
                if let Some(b) = match_const_pattern(alt, val) {
                    return Some(b);
                }
            }
            None
        }
        _ => None, // checker rejects unsupported patterns
    }
}

fn eval_unary(
    op: crate::ast::UnOp,
    v: &ConstValue,
    span: Span,
) -> Result<ConstValue, Diagnostic> {
    use crate::ast::UnOp;
    match (op, v) {
        (UnOp::Neg, ConstValue::Int(i)) => i
            .checked_neg()
            .map(ConstValue::Int)
            .ok_or_else(|| Diagnostic::new(
                "[E_CONST_FN_EVAL_OVERFLOW] integer negation overflow \
                 (i64::MIN) (D199).".to_string(),
                span,
            )),
        (UnOp::Neg, ConstValue::Float(f)) => Ok(ConstValue::Float(-f)),
        (UnOp::Not, ConstValue::Bool(b)) => Ok(ConstValue::Bool(!b)),
        _ => Err(Diagnostic::new(
            format!(
                "[E_CONST_FN_EVAL_PANIC] unary operator does not apply to \
                 {} (D199).",
                v.type_name()
            ),
            span,
        )),
    }
}

fn eval_binary(
    op: crate::ast::BinOp,
    l: &ConstValue,
    r: &ConstValue,
    span: Span,
) -> Result<ConstValue, Diagnostic> {
    use crate::ast::BinOp;
    match (l, r) {
        (ConstValue::Int(a), ConstValue::Int(b)) => match op {
            BinOp::Add => a.checked_add(*b).map(ConstValue::Int).ok_or_else(||
                overflow_err("addition", span)),
            BinOp::Sub => a.checked_sub(*b).map(ConstValue::Int).ok_or_else(||
                overflow_err("subtraction", span)),
            BinOp::Mul => a.checked_mul(*b).map(ConstValue::Int).ok_or_else(||
                overflow_err("multiplication", span)),
            BinOp::Div => {
                if *b == 0 {
                    Err(div_zero_err(span))
                } else {
                    a.checked_div(*b).map(ConstValue::Int).ok_or_else(||
                        overflow_err("division", span))
                }
            }
            BinOp::Mod => {
                if *b == 0 {
                    Err(div_zero_err(span))
                } else {
                    a.checked_rem(*b).map(ConstValue::Int).ok_or_else(||
                        overflow_err("modulo", span))
                }
            }
            BinOp::BitAnd => Ok(ConstValue::Int(a & b)),
            BinOp::BitOr => Ok(ConstValue::Int(a | b)),
            BinOp::BitXor => Ok(ConstValue::Int(a ^ b)),
            BinOp::Shl => shl_checked(*a, *b, span),
            BinOp::Shr => shr_checked(*a, *b, span),
            BinOp::Eq => Ok(ConstValue::Bool(a == b)),
            BinOp::Neq => Ok(ConstValue::Bool(a != b)),
            BinOp::Lt => Ok(ConstValue::Bool(a < b)),
            BinOp::Le => Ok(ConstValue::Bool(a <= b)),
            BinOp::Gt => Ok(ConstValue::Bool(a > b)),
            BinOp::Ge => Ok(ConstValue::Bool(a >= b)),
            _ => Err(unsupported_bin_err(op, "int", "int", span)),
        },
        (ConstValue::Float(a), ConstValue::Float(b)) => match op {
            BinOp::Add => Ok(ConstValue::Float(a + b)),
            BinOp::Sub => Ok(ConstValue::Float(a - b)),
            BinOp::Mul => Ok(ConstValue::Float(a * b)),
            BinOp::Div => {
                if *b == 0.0 {
                    Err(div_zero_err(span))
                } else {
                    Ok(ConstValue::Float(a / b))
                }
            }
            BinOp::Eq => Ok(ConstValue::Bool(a == b)),
            BinOp::Neq => Ok(ConstValue::Bool(a != b)),
            BinOp::Lt => Ok(ConstValue::Bool(a < b)),
            BinOp::Le => Ok(ConstValue::Bool(a <= b)),
            BinOp::Gt => Ok(ConstValue::Bool(a > b)),
            BinOp::Ge => Ok(ConstValue::Bool(a >= b)),
            _ => Err(unsupported_bin_err(op, "f64", "f64", span)),
        },
        (ConstValue::Bool(a), ConstValue::Bool(b)) => match op {
            BinOp::And => Ok(ConstValue::Bool(*a && *b)),
            BinOp::Or => Ok(ConstValue::Bool(*a || *b)),
            BinOp::Eq => Ok(ConstValue::Bool(a == b)),
            BinOp::Neq => Ok(ConstValue::Bool(a != b)),
            _ => Err(unsupported_bin_err(op, "bool", "bool", span)),
        },
        (ConstValue::Str(a), ConstValue::Str(b)) => match op {
            BinOp::Add => Ok(ConstValue::Str(format!("{}{}", a, b))),
            BinOp::Eq => Ok(ConstValue::Bool(a == b)),
            BinOp::Neq => Ok(ConstValue::Bool(a != b)),
            BinOp::Lt => Ok(ConstValue::Bool(a < b)),
            BinOp::Le => Ok(ConstValue::Bool(a <= b)),
            BinOp::Gt => Ok(ConstValue::Bool(a > b)),
            BinOp::Ge => Ok(ConstValue::Bool(a >= b)),
            _ => Err(unsupported_bin_err(op, "str", "str", span)),
        },
        (ConstValue::Char(a), ConstValue::Char(b)) => match op {
            BinOp::Eq => Ok(ConstValue::Bool(a == b)),
            BinOp::Neq => Ok(ConstValue::Bool(a != b)),
            BinOp::Lt => Ok(ConstValue::Bool(a < b)),
            BinOp::Le => Ok(ConstValue::Bool(a <= b)),
            BinOp::Gt => Ok(ConstValue::Bool(a > b)),
            BinOp::Ge => Ok(ConstValue::Bool(a >= b)),
            _ => Err(unsupported_bin_err(op, "char", "char", span)),
        },
        _ => Err(Diagnostic::new(
            format!(
                "[E_CONST_FN_EVAL_PANIC] binary `{:?}` between {} and {} — \
                 mixed-type operations not supported в const fn V1 (D199).",
                op, l.type_name(), r.type_name()
            ),
            span,
        )),
    }
}

fn shl_checked(a: i64, b: i64, span: Span) -> Result<ConstValue, Diagnostic> {
    if b < 0 || b >= 64 {
        return Err(Diagnostic::new(
            format!(
                "[E_CONST_FN_EVAL_OVERFLOW] shift-left amount {} out of range \
                 [0..63] for i64 (D199).",
                b
            ),
            span,
        ));
    }
    a.checked_shl(b as u32)
        .map(ConstValue::Int)
        .ok_or_else(|| overflow_err("shift-left", span))
}

fn shr_checked(a: i64, b: i64, span: Span) -> Result<ConstValue, Diagnostic> {
    if b < 0 || b >= 64 {
        return Err(Diagnostic::new(
            format!(
                "[E_CONST_FN_EVAL_OVERFLOW] shift-right amount {} out of range \
                 [0..63] for i64 (D199).",
                b
            ),
            span,
        ));
    }
    a.checked_shr(b as u32)
        .map(ConstValue::Int)
        .ok_or_else(|| overflow_err("shift-right", span))
}

fn overflow_err(op_name: &str, span: Span) -> Diagnostic {
    Diagnostic::new(
        format!(
            "[E_CONST_FN_EVAL_OVERFLOW] integer {} overflow в const fn evaluation \
             (i64 wrap-around treated as overflow in const context; not silent) \
             (D199).",
            op_name
        ),
        span,
    )
}

fn div_zero_err(span: Span) -> Diagnostic {
    Diagnostic::new(
        "[E_CONST_FN_DIV_ZERO] division / modulo by zero в const fn evaluation \
         (D199).".to_string(),
        span,
    )
}

fn unsupported_bin_err(
    op: crate::ast::BinOp,
    lt: &str,
    rt: &str,
    span: Span,
) -> Diagnostic {
    Diagnostic::new(
        format!(
            "[E_CONST_FN_EVAL_PANIC] binary `{:?}` between {} and {} not \
             supported в const fn V1 (D199).",
            op, lt, rt
        ),
        span,
    )
}

fn eval_as_cast(
    v: &ConstValue,
    target: &crate::ast::TypeRef,
    span: Span,
) -> Result<ConstValue, Diagnostic> {
    // Extract simple type name из target (V1: только primitive casts).
    let target_name = simple_type_name(target).ok_or_else(|| Diagnostic::new(
        "[E_CONST_FN_EVAL_PANIC] complex type в `as`-cast — V1 supports \
         only primitive casts (int/i32/i16/i8/i64/u8/u16/u32/u64/f32/f64/\
         bool/char/str) (D199).".to_string(),
        span,
    ))?;
    match (v, target_name.as_str()) {
        // Identity casts.
        (ConstValue::Int(_), "int" | "i64") => Ok(v.clone()),
        (ConstValue::Float(_), "f64") => Ok(v.clone()),
        (ConstValue::Bool(_), "bool") => Ok(v.clone()),
        (ConstValue::Char(_), "char") => Ok(v.clone()),
        (ConstValue::Str(_), "str") => Ok(v.clone()),
        // Char ↔ Int.
        (ConstValue::Char(c), "int" | "i64" | "i32" | "u32" | "u64") => {
            Ok(ConstValue::Int(*c as i64))
        }
        (ConstValue::Int(i), "char") => {
            if *i < 0 || *i > 0x10FFFF {
                Err(Diagnostic::new(
                    format!(
                        "[E_CONST_FN_EVAL_OVERFLOW] int {} not a valid char \
                         codepoint (D199).",
                        i
                    ),
                    span,
                ))
            } else {
                Ok(ConstValue::Char(*i as u32))
            }
        }
        // Int narrowing — V1 без silent truncation.
        (ConstValue::Int(i), "i32") => {
            if *i < i32::MIN as i64 || *i > i32::MAX as i64 {
                Err(overflow_err("i32 cast", span))
            } else {
                Ok(ConstValue::Int(*i))
            }
        }
        (ConstValue::Int(i), "i16") => {
            if *i < i16::MIN as i64 || *i > i16::MAX as i64 {
                Err(overflow_err("i16 cast", span))
            } else {
                Ok(ConstValue::Int(*i))
            }
        }
        (ConstValue::Int(i), "i8") => {
            if *i < i8::MIN as i64 || *i > i8::MAX as i64 {
                Err(overflow_err("i8 cast", span))
            } else {
                Ok(ConstValue::Int(*i))
            }
        }
        (ConstValue::Int(i), "u8") => {
            if *i < 0 || *i > u8::MAX as i64 {
                Err(overflow_err("u8 cast", span))
            } else {
                Ok(ConstValue::Int(*i))
            }
        }
        (ConstValue::Int(i), "u16") => {
            if *i < 0 || *i > u16::MAX as i64 {
                Err(overflow_err("u16 cast", span))
            } else {
                Ok(ConstValue::Int(*i))
            }
        }
        (ConstValue::Int(i), "u32") => {
            if *i < 0 || *i > u32::MAX as i64 {
                Err(overflow_err("u32 cast", span))
            } else {
                Ok(ConstValue::Int(*i))
            }
        }
        (ConstValue::Int(i), "u64") => {
            if *i < 0 {
                Err(overflow_err("u64 cast (negative)", span))
            } else {
                Ok(ConstValue::Int(*i))
            }
        }
        // Int ↔ Float.
        (ConstValue::Int(i), "f64" | "f32") => Ok(ConstValue::Float(*i as f64)),
        (ConstValue::Float(f), "int" | "i64") => {
            if !f.is_finite() {
                Err(Diagnostic::new(
                    "[E_CONST_FN_EVAL_OVERFLOW] non-finite float (NaN/Inf) cast \
                     to int (D199).".to_string(),
                    span,
                ))
            } else if *f < i64::MIN as f64 || *f > i64::MAX as f64 {
                Err(overflow_err("float→int cast", span))
            } else {
                Ok(ConstValue::Int(*f as i64))
            }
        }
        // Bool ↔ Int (convention: false=0, true=1).
        (ConstValue::Bool(b), "int" | "i64" | "i32" | "i16" | "i8" | "u8" | "u16" | "u32" | "u64") => {
            Ok(ConstValue::Int(if *b { 1 } else { 0 }))
        }
        _ => Err(Diagnostic::new(
            format!(
                "[E_CONST_FN_EVAL_PANIC] `as`-cast {} → {} not supported в \
                 const fn V1 (D199).",
                v.type_name(),
                target_name
            ),
            span,
        )),
    }
}

fn simple_type_name(t: &crate::ast::TypeRef) -> Option<String> {
    match t {
        crate::ast::TypeRef::Named { path, generics, .. } if generics.is_empty() && path.len() == 1 => {
            Some(path[0].clone())
        }
        _ => None,
    }
}

fn check_value_matches_type(
    v: &ConstValue,
    declared: &crate::ast::TypeRef,
    span: Span,
) -> Result<(), Diagnostic> {
    let name = match simple_type_name(declared) {
        Some(n) => n,
        None => return Ok(()), // Conservative: skip non-trivial types.
    };
    let ok = match (v, name.as_str()) {
        (ConstValue::Int(_), "int" | "i64" | "i32" | "i16" | "i8"
            | "u8" | "u16" | "u32" | "u64") => true,
        (ConstValue::Float(_), "f64" | "f32") => true,
        (ConstValue::Bool(_), "bool") => true,
        (ConstValue::Char(_), "char") => true,
        (ConstValue::Str(_), "str") => true,
        (ConstValue::Unit, "Unit") => true,
        _ => false,
    };
    if !ok {
        return Err(Diagnostic::new(
            format!(
                "[E_CONST_FN_EVAL_PANIC] param expected type `{}`, got value \
                 of type `{}` (D199).",
                name, v.type_name()
            ),
            span,
        ));
    }
    Ok(())
}

/// Extract `ConstValue` из literal Expr (used by AST rewriter to check
/// if an arg is constexpr-eligible without invoking full evaluator).
/// Returns `None` если expr — не литерал.
pub fn try_literal_to_value(expr: &Expr) -> Option<ConstValue> {
    match &expr.kind {
        ExprKind::IntLit(v) => Some(ConstValue::Int(*v)),
        ExprKind::FloatLit(v) => Some(ConstValue::Float(*v)),
        ExprKind::StrLit(v) => Some(ConstValue::Str(v.clone())),
        ExprKind::BoolLit(v) => Some(ConstValue::Bool(*v)),
        ExprKind::CharLit(v) => Some(ConstValue::Char(*v)),
        ExprKind::UnitLit => Some(ConstValue::Unit),
        ExprKind::Unary { .. } | ExprKind::Binary { .. } | ExprKind::As(..) => {
            // Higher-level constexpr eval handled by caller via full evaluator;
            // try_literal_to_value handles only direct literals.
            None
        }
        _ => None,
    }
}

#[allow(dead_code)]
fn _pattern_ident_marker(_p: &Pattern) {
    // Reserved for future scope-local pattern handling (no use в V1).
}

// =========================================================================
// AST rewriter — replaces const fn call sites с literal expressions and
// strips const fn declarations from codegen output (Plan 114.4.2 Ф.3).
// =========================================================================

/// Walks `module` and:
/// 1) Evaluates every `Expr::Call` где callee resolves к const fn from
///    the registry, replacing it с literal `Expr` of the evaluated
///    value (overflow / div-zero / etc. → Diagnostic).
/// 2) Removes const fn `FnDecl`s из `module.items` AND each peer's
///    `items_here` — codegen never sees them, no runtime symbol emitted.
///
/// Returns the collected diagnostics (any const-fn evaluation errors).
/// Caller decides fail-fast vs. collect-all.
///
/// Pipeline placement: AFTER `types::check_module` (so V1 subset
/// already enforced), BEFORE `desugar::desugar_module` (so subsequent
/// passes see only literals и no const fn).
pub fn rewrite_const_fn_calls(module: &mut crate::ast::Module) -> Vec<Diagnostic> {
    use crate::ast::{Item, Module};
    let mut errors: Vec<Diagnostic> = Vec::new();

    // Two-stage borrow: clone fn registry into a self-owning structure
    // so we can hold the immutable view while we mutate other parts of
    // the module. Const fn count в реальных модулях невелик; clone
    // overhead minimal.
    let const_fn_decls: Vec<crate::ast::FnDecl> = module.items.iter()
        .filter_map(|it| match it {
            Item::Fn(fd) => {
                let is_const = fd.return_is_const || fd.params.iter().any(|p| p.is_const);
                if is_const { Some(fd.clone()) } else { None }
            }
            _ => None,
        })
        .collect();
    if const_fn_decls.is_empty() {
        return errors;
    }
    let mut owned = OwnedEvaluator::new(const_fn_decls);

    fn walk_module(
        m: &mut Module,
        ev: &mut OwnedEvaluator,
        errors: &mut Vec<Diagnostic>,
    ) {
        for item in &mut m.items {
            walk_item(item, ev, errors);
        }
        for pf in &mut m.peer_files {
            for item in &mut pf.items_here {
                walk_item(item, ev, errors);
            }
        }
    }

    walk_module(module, &mut owned, &mut errors);

    // Drop const fn declarations from module.items + peer_files.items_here.
    let const_names = owned.names();
    module.items.retain(|it| match it {
        Item::Fn(fd) => !const_names.contains(&fd.name),
        _ => true,
    });
    for pf in &mut module.peer_files {
        pf.items_here.retain(|it| match it {
            Item::Fn(fd) => !const_names.contains(&fd.name),
            _ => true,
        });
    }
    errors
}

/// Self-owning evaluator wrapper — holds `Vec<FnDecl>` cloned from the
/// module, then exposes `&FnDecl` borrows internally. Avoids the
/// borrow-checker conflict of holding `&FnDecl` from `module.items`
/// while mutating other AST nodes.
struct OwnedEvaluator {
    fns: HashMap<String, crate::ast::FnDecl>,
    memo: HashMap<(String, Vec<ConstValue>), ConstValue>,
}

impl OwnedEvaluator {
    fn new(decls: Vec<crate::ast::FnDecl>) -> Self {
        let mut fns = HashMap::new();
        for fd in decls {
            fns.insert(fd.name.clone(), fd);
        }
        Self { fns, memo: HashMap::new() }
    }
    fn names(&self) -> std::collections::HashSet<String> {
        self.fns.keys().cloned().collect()
    }
    fn is_const_fn(&self, name: &str) -> bool {
        self.fns.contains_key(name)
    }
    fn eval_call(
        &mut self,
        name: &str,
        args: Vec<ConstValue>,
        call_span: Span,
    ) -> Result<ConstValue, Diagnostic> {
        self.eval_call_inner(name, args, call_span, 0)
    }
    fn eval_call_inner(
        &mut self,
        name: &str,
        args: Vec<ConstValue>,
        call_span: Span,
        depth: usize,
    ) -> Result<ConstValue, Diagnostic> {
        if depth >= MAX_EVAL_DEPTH {
            return Err(Diagnostic::new(
                format!(
                    "[E_CONST_FN_EVAL_PANIC] const fn evaluation exceeded depth \
                     limit {} в call to `{}` (D199).",
                    MAX_EVAL_DEPTH, name
                ),
                call_span,
            ));
        }
        let key = (name.to_string(), args.clone());
        if let Some(v) = self.memo.get(&key) {
            return Ok(v.clone());
        }
        let fd = self.fns.get(name).cloned().ok_or_else(|| {
            Diagnostic::new(
                format!("[E_CONST_FN_EVAL_PANIC] const fn `{}` not found (D199).", name),
                call_span,
            )
        })?;
        if fd.params.len() != args.len() {
            return Err(Diagnostic::new(
                format!(
                    "[E_CONST_FN_NON_CONST_ARG] arity mismatch calling const fn \
                     `{}` — expected {}, got {} (D199).",
                    name, fd.params.len(), args.len()
                ),
                call_span,
            ));
        }
        let mut env: HashMap<String, ConstValue> = HashMap::new();
        for (p, v) in fd.params.iter().zip(args.iter()) {
            check_value_matches_type(v, &p.ty, call_span)?;
            env.insert(p.name.clone(), v.clone());
        }
        let result = match &fd.body {
            FnBody::Expr(e) => self.eval_expr(e, &env, name, depth + 1)?,
            FnBody::Block(b) => self.eval_block(b, &env, name, depth + 1)?,
            FnBody::External => {
                return Err(Diagnostic::new(
                    format!(
                        "[E_CONST_FN_EXTERNAL] external const fn `{}` cannot be \
                         comptime-evaluated (D199).", name
                    ),
                    fd.span,
                ));
            }
        };
        self.memo.insert(key, result.clone());
        Ok(result)
    }
    fn eval_block(
        &mut self,
        block: &Block,
        env: &HashMap<String, ConstValue>,
        current_fn: &str,
        depth: usize,
    ) -> Result<ConstValue, Diagnostic> {
        let mut env = env.clone();
        for st in &block.stmts {
            match st {
                Stmt::Const(cd) => {
                    let v = self.eval_expr(&cd.value, &env, current_fn, depth)?;
                    env.insert(cd.name.clone(), v);
                }
                Stmt::Expr(e) => {
                    return self.eval_expr(e, &env, current_fn, depth);
                }
                Stmt::Return { value: Some(e), .. } => {
                    return self.eval_expr(e, &env, current_fn, depth);
                }
                Stmt::Return { value: None, span } => {
                    return Err(Diagnostic::new(
                        "[E_CONST_FN_EVAL_PANIC] bare `return` без значения (D199).".to_string(),
                        *span,
                    ));
                }
                _ => {
                    return Err(Diagnostic::new(
                        "[E_CONST_FN_EVAL_PANIC] unexpected stmt during eval (D199).".to_string(),
                        block.span,
                    ));
                }
            }
        }
        if let Some(trail) = &block.trailing {
            return self.eval_expr(trail, &env, current_fn, depth);
        }
        Err(Diagnostic::new(
            "[E_CONST_FN_EVAL_PANIC] const fn body produced no value (D199).".to_string(),
            block.span,
        ))
    }
    fn eval_expr(
        &mut self,
        expr: &Expr,
        env: &HashMap<String, ConstValue>,
        current_fn: &str,
        depth: usize,
    ) -> Result<ConstValue, Diagnostic> {
        match &expr.kind {
            ExprKind::IntLit(v) => Ok(ConstValue::Int(*v)),
            ExprKind::FloatLit(v) => Ok(ConstValue::Float(*v)),
            ExprKind::StrLit(v) => Ok(ConstValue::Str(v.clone())),
            ExprKind::BoolLit(v) => Ok(ConstValue::Bool(*v)),
            ExprKind::CharLit(v) => Ok(ConstValue::Char(*v)),
            ExprKind::UnitLit => Ok(ConstValue::Unit),
            ExprKind::Ident(name) => env
                .get(name)
                .cloned()
                .ok_or_else(|| Diagnostic::new(
                    format!(
                        "[E_CONST_FN_EVAL_PANIC] unbound identifier `{}` (D199).",
                        name
                    ),
                    expr.span,
                )),
            ExprKind::Unary { op, operand } => {
                let v = self.eval_expr(operand, env, current_fn, depth)?;
                eval_unary(*op, &v, expr.span)
            }
            ExprKind::Binary { op, left, right } => {
                let l = self.eval_expr(left, env, current_fn, depth)?;
                let r = self.eval_expr(right, env, current_fn, depth)?;
                eval_binary(*op, &l, &r, expr.span)
            }
            ExprKind::As(inner, target) => {
                let v = self.eval_expr(inner, env, current_fn, depth)?;
                eval_as_cast(&v, target, expr.span)
            }
            ExprKind::Call { func, args, trailing: None } => {
                let callee_name = if let ExprKind::Ident(n) = &func.kind {
                    n.clone()
                } else {
                    return Err(Diagnostic::new(
                        "[E_CONST_FN_EVAL_PANIC] non-ident callee (D199).".to_string(),
                        expr.span,
                    ));
                };
                let mut arg_vals = Vec::with_capacity(args.len());
                for a in args {
                    if let crate::ast::CallArg::Item(e) = a {
                        arg_vals.push(self.eval_expr(e, env, current_fn, depth)?);
                    } else {
                        return Err(Diagnostic::new(
                            "[E_CONST_FN_EVAL_PANIC] non-positional arg (D199).".to_string(),
                            expr.span,
                        ));
                    }
                }
                self.eval_call_inner(&callee_name, arg_vals, expr.span, depth)
            }
            ExprKind::Block(b) => self.eval_block(b, env, current_fn, depth),
            // Plan 114.4.3 Ф.1 (V2): If — evaluate cond + branch.
            ExprKind::If { cond, then, else_ } => {
                let c = self.eval_expr(cond, env, current_fn, depth)?;
                let b = match c {
                    ConstValue::Bool(b) => b,
                    _ => return Err(Diagnostic::new(
                        format!(
                            "[E_CONST_FN_EVAL_PANIC] if-condition не bool (got {}) (D199).",
                            c.type_name()
                        ),
                        cond.span,
                    )),
                };
                if b {
                    self.eval_block(then, env, current_fn, depth)
                } else {
                    match else_ {
                        Some(crate::ast::ElseBranch::Block(blk)) =>
                            self.eval_block(blk, env, current_fn, depth),
                        Some(crate::ast::ElseBranch::If(ie)) =>
                            self.eval_expr(ie, env, current_fn, depth),
                        None => Err(Diagnostic::new(
                            "[E_CONST_FN_EVAL_PANIC] if без else в const fn body \
                             (D199 V2).".to_string(),
                            expr.span,
                        )),
                    }
                }
            }
            // Plan 114.4.3 Ф.1 (V2): Match — find matching arm.
            ExprKind::Match { scrutinee, arms } => {
                let sv = self.eval_expr(scrutinee, env, current_fn, depth)?;
                for arm in arms {
                    if let Some(bindings) = match_const_pattern(&arm.pattern, &sv) {
                        let mut new_env = env.clone();
                        for (k, v) in bindings { new_env.insert(k, v); }
                        if let Some(g) = &arm.guard {
                            let gv = self.eval_expr(g, &new_env, current_fn, depth)?;
                            if !matches!(gv, ConstValue::Bool(true)) { continue; }
                        }
                        return match &arm.body {
                            crate::ast::MatchArmBody::Expr(e) =>
                                self.eval_expr(e, &new_env, current_fn, depth),
                            crate::ast::MatchArmBody::Block(b) =>
                                self.eval_block(b, &new_env, current_fn, depth),
                        };
                    }
                }
                Err(Diagnostic::new(
                    format!(
                        "[E_CONST_FN_MATCH_EXHAUSTIVE] match не covered scrutinee \
                         value {:?} (D199 V2). Add wildcard `_` arm.",
                        sv
                    ),
                    expr.span,
                ))
            }
            _ => Err(Diagnostic::new(
                "[E_CONST_FN_EVAL_PANIC] expression form not supported (D199).".to_string(),
                expr.span,
            )),
        }
    }
}

fn walk_item(
    item: &mut crate::ast::Item,
    ev: &mut OwnedEvaluator,
    errors: &mut Vec<Diagnostic>,
) {
    use crate::ast::Item;
    match item {
        Item::Fn(fd) => {
            // Plan 114.4.2 D199: const fn bodies will be dropped — skip them.
            // Их body interpreted at eval time через evaluator, не AST rewrite.
            // Default params (D102) и runtime fn body — нужно walk'ать.
            let is_const_fn = fd.return_is_const
                || fd.params.iter().any(|p| p.is_const);
            if is_const_fn {
                return;
            }
            for p in &mut fd.params {
                if let Some(def) = &mut p.default {
                    walk_expr(def, ev, errors);
                }
            }
            match &mut fd.body {
                FnBody::Expr(e) => walk_expr(e, ev, errors),
                FnBody::Block(b) => walk_block(b, ev, errors),
                FnBody::External => {}
            }
        }
        Item::Const(c) => walk_expr(&mut c.value, ev, errors),
        Item::Let(l) => walk_expr(&mut l.value, ev, errors),
        Item::Test(t) => walk_block(&mut t.body, ev, errors),
        Item::Bench(b) => {
            for s in &mut b.setup { walk_stmt(s, ev, errors); }
            walk_block(&mut b.measure_body, ev, errors);
            for s in &mut b.teardown { walk_stmt(s, ev, errors); }
        }
        Item::Type(td) => {
            // Plan 114.4.1 D200: assoc const RHS могут вызывать const fn.
            for ac in &mut td.assoc_consts {
                walk_expr(&mut ac.value, ev, errors);
            }
        }
        Item::Lemma(_) => {}
    }
}

fn walk_block(
    b: &mut crate::ast::Block,
    ev: &mut OwnedEvaluator,
    errors: &mut Vec<Diagnostic>,
) {
    for s in &mut b.stmts {
        walk_stmt(s, ev, errors);
    }
    if let Some(t) = &mut b.trailing {
        walk_expr(t, ev, errors);
    }
}

fn walk_stmt(
    s: &mut Stmt,
    ev: &mut OwnedEvaluator,
    errors: &mut Vec<Diagnostic>,
) {
    match s {
        Stmt::Let(d) => walk_expr(&mut d.value, ev, errors),
        Stmt::Const(d) => walk_expr(&mut d.value, ev, errors),
        Stmt::Expr(e) => walk_expr(e, ev, errors),
        Stmt::Assign { target, value, .. } => {
            walk_expr(target, ev, errors);
            walk_expr(value, ev, errors);
        }
        Stmt::Return { value, .. } => {
            if let Some(v) = value { walk_expr(v, ev, errors); }
        }
        Stmt::Throw { value, .. } => walk_expr(value, ev, errors),
        Stmt::Break(_) | Stmt::Continue(_) => {}
        Stmt::Defer { body, .. } | Stmt::ErrDefer { body, .. }
        | Stmt::OkDefer { body, .. } | Stmt::DeferWithResult { body, .. } => {
            walk_expr(body, ev, errors);
        }
        Stmt::ConsumeScope { init, body, .. } => {
            walk_expr(init, ev, errors);
            walk_block(body, ev, errors);
        }
        Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => {
            walk_expr(expr, ev, errors);
        }
        Stmt::Apply { .. } | Stmt::Calc { .. } | Stmt::Reveal { .. } => {}
    }
}

fn walk_expr(
    e: &mut Expr,
    ev: &mut OwnedEvaluator,
    errors: &mut Vec<Diagnostic>,
) {
    walk_children(e, ev, errors);
    // Try replacement at this node.
    if let ExprKind::Call { func, args, trailing: None } = &e.kind {
        if let ExprKind::Ident(name) = &func.kind {
            if ev.is_const_fn(name) {
                // Collect arg values — only literal args eligible после
                // children walk (which already replaced nested const fn calls).
                let mut arg_vals: Vec<ConstValue> = Vec::with_capacity(args.len());
                let mut all_literal = true;
                for a in args {
                    match a {
                        crate::ast::CallArg::Item(ae) => {
                            if let Some(v) = try_literal_to_value(ae) {
                                arg_vals.push(v);
                            } else {
                                all_literal = false;
                                break;
                            }
                        }
                        _ => {
                            all_literal = false;
                            break;
                        }
                    }
                }
                if !all_literal {
                    errors.push(Diagnostic::new(
                        format!(
                            "[E_CONST_FN_NON_CONST_ARG] call to const fn `{}` \
                             with non-constexpr argument(s) — all args must be \
                             literals или other const fn calls с literal args \
                             (D199).",
                            name
                        ),
                        e.span,
                    ));
                    return;
                }
                let name_owned = name.clone();
                let span = e.span;
                match ev.eval_call(&name_owned, arg_vals, span) {
                    Ok(v) => {
                        let lit = v.to_literal_expr(span);
                        *e = lit;
                    }
                    Err(d) => {
                        errors.push(d);
                    }
                }
            }
        }
    }
}

fn walk_children(
    e: &mut Expr,
    ev: &mut OwnedEvaluator,
    errors: &mut Vec<Diagnostic>,
) {
    use crate::ast::{ArrayElem, CallArg, ElseBranch, MapElem, MatchArm};
    match &mut e.kind {
        ExprKind::IntLit(_) | ExprKind::FloatLit(_) | ExprKind::StrLit(_)
        | ExprKind::BoolLit(_) | ExprKind::CharLit(_) | ExprKind::UnitLit
        | ExprKind::Ident(_) | ExprKind::Path(_) | ExprKind::SelfAccess => {}
        ExprKind::InterpolatedStr { parts } => {
            for p in parts {
                if let crate::ast::InterpStrPart::Expr(ex) = p {
                    walk_expr(ex, ev, errors);
                }
            }
        }
        ExprKind::ArrayLit(elems) => {
            for el in elems {
                match el {
                    ArrayElem::Item(x) | ArrayElem::Spread(x) => walk_expr(x, ev, errors),
                }
            }
        }
        ExprKind::MapLit { elems, .. } => {
            for el in elems {
                match el {
                    MapElem::Pair(k, v) => {
                        walk_expr(k, ev, errors);
                        walk_expr(v, ev, errors);
                    }
                    MapElem::Spread(s) => walk_expr(s, ev, errors),
                }
            }
        }
        ExprKind::RecordLit { fields, .. } => {
            for f in fields {
                if let Some(v) = &mut f.value {
                    walk_expr(v, ev, errors);
                }
            }
        }
        ExprKind::TupleLit(items) => {
            for x in items { walk_expr(x, ev, errors); }
        }
        ExprKind::Member { obj, .. } => walk_expr(obj, ev, errors),
        ExprKind::Index { obj, index } => {
            walk_expr(obj, ev, errors);
            walk_expr(index, ev, errors);
        }
        ExprKind::TurboFish { base, .. } => walk_expr(base, ev, errors),
        ExprKind::Call { func, args, trailing } => {
            walk_expr(func, ev, errors);
            for a in args {
                match a {
                    CallArg::Item(x) | CallArg::Spread(x) => walk_expr(x, ev, errors),
                    CallArg::Named { value, .. } => walk_expr(value, ev, errors),
                }
            }
            if let Some(t) = trailing {
                match t {
                    crate::ast::Trailing::Block(b) => walk_block(b, ev, errors),
                    crate::ast::Trailing::Fn(sb) => walk_fn_body(&mut sb.body, ev, errors),
                    crate::ast::Trailing::LegacyBlockWithParams(tb) => walk_block(&mut tb.body, ev, errors),
                }
            }
        }
        ExprKind::Try(x) | ExprKind::Bang(x) => walk_expr(x, ev, errors),
        ExprKind::Coalesce(a, b) => { walk_expr(a, ev, errors); walk_expr(b, ev, errors); }
        ExprKind::As(x, _) | ExprKind::Is(x, _) => walk_expr(x, ev, errors),
        ExprKind::Binary { left, right, .. } => {
            walk_expr(left, ev, errors); walk_expr(right, ev, errors);
        }
        ExprKind::Unary { operand, .. } => walk_expr(operand, ev, errors),
        ExprKind::If { cond, then, else_ } => {
            walk_expr(cond, ev, errors);
            walk_block(then, ev, errors);
            if let Some(eb) = else_ {
                match eb {
                    ElseBranch::Block(b) => walk_block(b, ev, errors),
                    ElseBranch::If(ie) => walk_expr(ie, ev, errors),
                }
            }
        }
        ExprKind::IfLet { scrutinee, then, else_, .. } => {
            walk_expr(scrutinee, ev, errors);
            walk_block(then, ev, errors);
            if let Some(eb) = else_ {
                match eb {
                    ElseBranch::Block(b) => walk_block(b, ev, errors),
                    ElseBranch::If(ie) => walk_expr(ie, ev, errors),
                }
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            walk_expr(scrutinee, ev, errors);
            for arm in arms {
                if let Some(g) = &mut arm.guard {
                    walk_expr(g, ev, errors);
                }
                walk_match_arm_body(arm, ev, errors);
            }
        }
        ExprKind::For { iter, body, .. } => {
            walk_expr(iter, ev, errors);
            walk_block(body, ev, errors);
        }
        ExprKind::ParallelFor { iter, body, .. } => {
            walk_expr(iter, ev, errors);
            walk_block(body, ev, errors);
        }
        ExprKind::While { cond, body, .. } => {
            walk_expr(cond, ev, errors);
            walk_block(body, ev, errors);
        }
        ExprKind::WhileLet { scrutinee, body, .. } => {
            walk_expr(scrutinee, ev, errors);
            walk_block(body, ev, errors);
        }
        ExprKind::Loop { body, .. } => walk_block(body, ev, errors),
        ExprKind::Block(b) => walk_block(b, ev, errors),
        ExprKind::Lambda { body, .. } => walk_expr(body, ev, errors),
        ExprKind::ClosureLight { body, .. } => walk_closure_body(body, ev, errors),
        ExprKind::ClosureFull(sb) => walk_fn_body(&mut sb.body, ev, errors),
        ExprKind::Spawn(b) => walk_expr(b, ev, errors),
        ExprKind::Supervised { body, .. } => walk_block(body, ev, errors),
        ExprKind::Detach(b) | ExprKind::Blocking(b) => walk_block(b, ev, errors),
        ExprKind::With { body, .. } => walk_block(body, ev, errors),
        ExprKind::Forbid { body, .. } => walk_block(body, ev, errors),
        ExprKind::Realtime { body, .. } => walk_block(body, ev, errors),
        ExprKind::Interrupt(opt) => {
            if let Some(x) = opt { walk_expr(x, ev, errors); }
        }
        ExprKind::HandlerLit { .. } | ExprKind::ProtocolLit { .. } => {}
        _ => {}
    }
}

fn walk_closure_body(
    cb: &mut crate::ast::ClosureBody,
    ev: &mut OwnedEvaluator,
    errors: &mut Vec<Diagnostic>,
) {
    match cb {
        crate::ast::ClosureBody::Expr(e) => walk_expr(e, ev, errors),
        crate::ast::ClosureBody::Block(b) => walk_block(b, ev, errors),
    }
}

fn walk_fn_body(
    body: &mut FnBody,
    ev: &mut OwnedEvaluator,
    errors: &mut Vec<Diagnostic>,
) {
    match body {
        FnBody::Expr(e) => walk_expr(e, ev, errors),
        FnBody::Block(b) => walk_block(b, ev, errors),
        FnBody::External => {}
    }
}

fn walk_match_arm_body(
    arm: &mut crate::ast::MatchArm,
    ev: &mut OwnedEvaluator,
    errors: &mut Vec<Diagnostic>,
) {
    match &mut arm.body {
        crate::ast::MatchArmBody::Expr(e) => walk_expr(e, ev, errors),
        crate::ast::MatchArmBody::Block(b) => walk_block(b, ev, errors),
    }
}
