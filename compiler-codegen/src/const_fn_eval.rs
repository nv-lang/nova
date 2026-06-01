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
            _ => Err(Diagnostic::new(
                "[E_CONST_FN_EVAL_PANIC] expression form not supported by const \
                 fn evaluator — checker should reject (D199).".to_string(),
                expr.span,
            )),
        }
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
