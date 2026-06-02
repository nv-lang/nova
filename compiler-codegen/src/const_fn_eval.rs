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
    /// Plan 114.4.4 Ф.4 V4 (D199 V4 amend): tuple structured value.
    /// Positional access via .0/.1/... evaluation. Match patterns:
    /// `(a, b)` destructuring.
    Tuple(Vec<ConstValue>),
    /// Plan 114.4.4 Ф.4 V4: variant value `VariantName(args)`.
    /// Match patterns: `Some(x)`, `None`, `Ok(v)`, `Err(e)`, etc.
    Variant(String, Vec<ConstValue>),
    /// Plan 114.4.4 Ф.4 V4: record value `Type { field: val, ... }`.
    /// Match patterns: `{ field: pat }`.
    Record(Vec<(String, ConstValue)>),
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
            (ConstValue::Tuple(a), ConstValue::Tuple(b)) => a == b,
            (ConstValue::Variant(na, va), ConstValue::Variant(nb, vb)) => na == nb && va == vb,
            (ConstValue::Record(a), ConstValue::Record(b)) => a == b,
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
            ConstValue::Tuple(items) => {
                6u8.hash(state);
                items.len().hash(state);
                for v in items { v.hash(state); }
            }
            ConstValue::Variant(name, args) => {
                7u8.hash(state);
                name.hash(state);
                args.len().hash(state);
                for v in args { v.hash(state); }
            }
            ConstValue::Record(fields) => {
                8u8.hash(state);
                fields.len().hash(state);
                for (k, v) in fields { k.hash(state); v.hash(state); }
            }
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
            // Plan 114.4.4 Ф.4: tuple → TupleLit с converted children.
            ConstValue::Tuple(items) => {
                let elems = items.iter().map(|v| v.to_literal_expr(span)).collect();
                ExprKind::TupleLit(elems)
            }
            // Plan 114.4.4 Ф.4: variant → Call expression `Name(args)`.
            ConstValue::Variant(name, args) => {
                let func = Box::new(Expr {
                    kind: ExprKind::Ident(name.clone()), span,
                });
                let call_args = args.iter().map(|v|
                    crate::ast::CallArg::Item(v.to_literal_expr(span))
                ).collect();
                ExprKind::Call { func, args: call_args, trailing: None }
            }
            // Plan 114.4.4 Ф.4: record → RecordLit с fields.
            ConstValue::Record(fields) => {
                let lit_fields = fields.iter().map(|(k, v)| {
                    crate::ast::RecordLitField {
                        name: k.clone(),
                        value: Some(v.to_literal_expr(span)),
                        is_spread: false,
                        at_shorthand: false,
                        span,
                    }
                }).collect();
                ExprKind::RecordLit {
                    type_name: None,
                    fields: lit_fields,
                    inferred_map_v: None,
                }
            }
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
            ConstValue::Tuple(_) => "tuple",
            ConstValue::Variant(_, _) => "variant",
            ConstValue::Record(_) => "record",
        }
    }
}

/// Recursion depth budget (defensive — checker rejects recursion, but
/// in case mutual cycle escapes detection, this stops the evaluator
/// from blowing the stack).
/// Plan 114.4.3 Ф.2 (D199 V2): recursion depth limit для const fn evaluator.
/// Raised from V1's 64 (defensive) to 256 (production — supports reasonable
/// factorial / fibonacci / mutual recursion fixtures без crashing).
/// При reach → E_CONST_FN_EVAL_DEPTH_EXCEEDED с suggestion increase OR
/// add memoization (which V1 уже has).
const MAX_EVAL_DEPTH: usize = 256;

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
                // Plan 114.4.3 Ф.3 V2: ONLY fully-const fns evaluated and
                // dropped from codegen. Mixed fns kept as runtime fns.
                let all_const_params = !fd.params.is_empty()
                    && fd.params.iter().all(|p| p.is_const);
                let is_fully_const = (all_const_params || fd.params.is_empty())
                    && fd.return_is_const;
                if is_fully_const {
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
        // Plan 114.4.4 Ф.1 (D199 V3): per-fn override depth limit
        // через `#fn_eval_max_depth(N)` attribute.
        let depth_limit = self.fns.get(name)
            .and_then(|fd| fd.fn_eval_max_depth)
            .map(|n| n as usize)
            .unwrap_or(MAX_EVAL_DEPTH);
        if depth >= depth_limit {
            return Err(Diagnostic::new(
                format!(
                    "[E_CONST_FN_EVAL_DEPTH_EXCEEDED] const fn evaluation exceeded \
                     depth limit {} в call to `{}` — likely runaway recursion. \
                     V2 supports recursion (Plan 114.4.3) — verify base case + \
                     memoization работают. Use `#fn_eval_max_depth(N)` attribute \
                     для override (V3 Plan 114.4.4 Ф.1). D199 V3.",
                    depth_limit, name
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
                // Callee: Ident OR TurboFish<Ident, ...> (generic const fn V2).
                let callee_name = match &func.kind {
                    ExprKind::Ident(n) => n.clone(),
                    ExprKind::TurboFish { base, .. } => {
                        if let ExprKind::Ident(n) = &base.kind {
                            n.clone()
                        } else {
                            return Err(Diagnostic::new(
                                "[E_CONST_FN_EVAL_PANIC] non-ident turbofish base \
                                 (D199).".to_string(),
                                expr.span,
                            ));
                        }
                    }
                    _ => return Err(Diagnostic::new(
                        "[E_CONST_FN_EVAL_PANIC] non-ident callee в const fn \
                         body — checker drift (D199).".to_string(),
                        expr.span,
                    )),
                };
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
    use crate::ast::{Pattern, Literal, VariantPatternKind};
    match pat {
        Pattern::Wildcard(_) => Some(Vec::new()),
        Pattern::Ident { name, is_mut: false, .. } => {
            // Bare Ident pattern может быть:
            // (1) unit variant matcher (e.g. `None` matches Variant("None", []))
            // (2) plain binding (fallback)
            if let ConstValue::Variant(vname, args) = val {
                if args.is_empty() && vname == name {
                    return Some(Vec::new());
                }
            }
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
        // Plan 114.4.4 Ф.4 V4: tuple destructuring pattern.
        Pattern::Tuple(pats, _) => {
            if let ConstValue::Tuple(items) = val {
                if pats.len() != items.len() { return None; }
                let mut bindings = Vec::new();
                for (p, v) in pats.iter().zip(items.iter()) {
                    let sub = match_const_pattern(p, v)?;
                    bindings.extend(sub);
                }
                Some(bindings)
            } else {
                None
            }
        }
        // Plan 114.4.4 Ф.4 V4: variant destructuring pattern.
        // `Some(x)`, `Ok(v)`, `Err(e)`, `Cons(h, ..)`, etc.
        Pattern::Variant { path, kind, .. } => {
            if let ConstValue::Variant(vname, args) = val {
                // Path's last segment matches variant name; bare variant
                // name (path.len()==1) covers common case Some/None/Ok/Err.
                let name = path.last().map(String::as_str).unwrap_or("");
                if name != vname { return None; }
                match kind {
                    VariantPatternKind::Unit => {
                        if args.is_empty() { Some(Vec::new()) } else { None }
                    }
                    VariantPatternKind::Tuple { patterns, rest } => {
                        if !rest && patterns.len() != args.len() { return None; }
                        if *rest && patterns.len() > args.len() { return None; }
                        let mut bindings = Vec::new();
                        for (p, v) in patterns.iter().zip(args.iter()) {
                            let sub = match_const_pattern(p, v)?;
                            bindings.extend(sub);
                        }
                        Some(bindings)
                    }
                }
            } else {
                None
            }
        }
        // Plan 114.4.4 Ф.4 V4: record destructuring pattern.
        Pattern::Record { fields, rest, .. } => {
            if let ConstValue::Record(rec_fields) = val {
                if !rest && fields.len() != rec_fields.len() { return None; }
                let mut bindings = Vec::new();
                for f in fields {
                    let v = rec_fields.iter().find(|(k, _)| k == &f.name).map(|(_, v)| v);
                    let v = match v { Some(v) => v, None => return None };
                    match &f.pattern {
                        Some(p) => {
                            let sub = match_const_pattern(p, v)?;
                            bindings.extend(sub);
                        }
                        None => {
                            // Shorthand `{ name }` — bind field value к field name.
                            bindings.push((f.name.clone(), v.clone()));
                        }
                    }
                }
                Some(bindings)
            } else {
                None
            }
        }
        Pattern::Binding { name, inner, .. } => {
            let mut sub = match_const_pattern(inner, val)?;
            sub.push((name.clone(), val.clone()));
            Some(sub)
        }
        _ => None,
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
/// Plan 114.4.4 Ф.5 V4: extract simple type name from TypeRef (для intrinsics).
pub fn simple_type_name_str(t: &crate::ast::TypeRef) -> Option<String> {
    match t {
        crate::ast::TypeRef::Named { path, generics, .. }
            if generics.is_empty() && path.len() == 1 =>
        {
            Some(path[0].clone())
        }
        _ => None,
    }
}

/// Plan 114.4.4 Ф.5 V4: sizeof[T]()/align_of[T]() — type layout lookup.
/// V4.0 supports primitive types only (hardcoded sizes per default 64-bit ABI).
/// Records/generics/sum-types → followup `[M-114.4.4-record-reflection]`.
/// `is_align`: true для align_of, false для sizeof.
pub fn type_size_or_align(t: &crate::ast::TypeRef, is_align: bool) -> Option<i64> {
    let name = simple_type_name_str(t)?;
    // Primitive type table (default 64-bit ABI, matches nova_rt).
    // align == size для primitives (natural alignment).
    let size = match name.as_str() {
        "int" | "i64" | "u64" | "f64" => 8,
        "i32" | "u32" | "f32" => 4,
        "i16" | "u16" => 2,
        "i8" | "u8" | "bool" => 1,
        "char" => 4, // Plan Q-char-literals: u32 codepoint.
        "str" => 16, // pointer + length (Plan 26 prelude).
        _ => return None,
    };
    let _ = is_align;
    Some(size)
}

pub fn try_literal_to_value(expr: &Expr) -> Option<ConstValue> {
    match &expr.kind {
        ExprKind::IntLit(v) => Some(ConstValue::Int(*v)),
        ExprKind::FloatLit(v) => Some(ConstValue::Float(*v)),
        ExprKind::StrLit(v) => Some(ConstValue::Str(v.clone())),
        ExprKind::BoolLit(v) => Some(ConstValue::Bool(*v)),
        ExprKind::CharLit(v) => Some(ConstValue::Char(*v)),
        ExprKind::UnitLit => Some(ConstValue::Unit),
        // Plan 114.4.4 Ф.4 V4: pre-eval constexpr Unary/Binary/As для args.
        ExprKind::Unary { op, operand } => {
            let v = try_literal_to_value(operand)?;
            eval_unary(*op, &v, expr.span).ok()
        }
        ExprKind::Binary { op, left, right } => {
            let l = try_literal_to_value(left)?;
            let r = try_literal_to_value(right)?;
            eval_binary(*op, &l, &r, expr.span).ok()
        }
        ExprKind::As(inner, target) => {
            let v = try_literal_to_value(inner)?;
            eval_as_cast(&v, target, expr.span).ok()
        }
        // Plan 114.4.4 Ф.4 V4: tuple/variant constructor literals.
        ExprKind::TupleLit(elems) => {
            let mut vs = Vec::with_capacity(elems.len());
            for e in elems { vs.push(try_literal_to_value(e)?); }
            Some(ConstValue::Tuple(vs))
        }
        ExprKind::Call { func, args, trailing: None } => {
            // Plan 114.4.4 Ф.5 V4: sizeof/align_of intrinsics — TurboFish callee.
            if let ExprKind::TurboFish { base, type_args } = &func.kind {
                if let ExprKind::Ident(n) = &base.kind {
                    if (n == "sizeof" || n == "align_of") && type_args.len() == 1 && args.is_empty() {
                        return type_size_or_align(&type_args[0], n == "align_of")
                            .map(ConstValue::Int);
                    }
                }
            }
            // Variant constructor `Name(args)` где Name uppercase.
            if let ExprKind::Ident(n) = &func.kind {
                if n.chars().next().map_or(false, |c| c.is_uppercase()) {
                    let mut vs = Vec::with_capacity(args.len());
                    for a in args {
                        if let crate::ast::CallArg::Item(e) = a {
                            vs.push(try_literal_to_value(e)?);
                        } else {
                            return None;
                        }
                    }
                    return Some(ConstValue::Variant(n.clone(), vs));
                }
            }
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
    let mut mixed_fns: Vec<(String, Vec<bool>, crate::ast::FnDecl)> = Vec::new();
    let const_fn_decls: Vec<crate::ast::FnDecl> = module.items.iter()
        .filter_map(|it| match it {
            Item::Fn(fd) => {
                // Plan 114.4.3 Ф.3 V2:
                // - Fully-const fn: ALL params const + const return. Walked + dropped.
                // - Mixed fn: any const param OR const return, NOT fully-const.
                //   Stays в codegen; const-args validated at call sites.
                let all_const_params = !fd.params.is_empty()
                    && fd.params.iter().all(|p| p.is_const);
                let is_fully_const = (all_const_params || fd.params.is_empty())
                    && fd.return_is_const;
                let any_const = fd.return_is_const || fd.params.iter().any(|p| p.is_const);
                if is_fully_const {
                    Some(fd.clone())
                } else if any_const {
                    // Mixed — collect for validation registration + V4.1 mono.
                    let flags: Vec<bool> = fd.params.iter().map(|p| p.is_const).collect();
                    mixed_fns.push((fd.name.clone(), flags, fd.clone()));
                    None
                } else {
                    None
                }
            }
            _ => None,
        })
        .collect();
    // Plan 114.4.4 Ф.5: walk module even без const fns — sizeof/align_of
    // intrinsics могут use'аться в module-level const RHS без const fn
    // decls (e.g. `const SIZE = sizeof[int]()`).
    let mut owned = OwnedEvaluator::new(const_fn_decls);
    for (name, flags, fd) in mixed_fns {
        owned.register_mixed(name, flags, fd);
    }
    // Plan 114.4.3 Ф.5 V2: first-class alias resolution.
    // Detect `const ALIAS = const_fn_name` pattern. Build alias map:
    // ALIAS → ORIGINAL_NAME. Substituted at call sites; alias decls
    // removed from items в retain phase.
    let mut const_fn_aliases: HashMap<String, String> = HashMap::new();
    {
        let const_fn_universe: std::collections::HashSet<String> = owned.fns
            .keys()
            .chain(owned.mixed_const_params.keys())
            .cloned()
            .collect();
        for item in &module.items {
            if let Item::Const(c) = item {
                if let ExprKind::Ident(target) = &c.value.kind {
                    if const_fn_universe.contains(target) {
                        const_fn_aliases.insert(c.name.clone(), target.clone());
                    }
                }
            }
        }
    }
    owned.aliases = const_fn_aliases;

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

    // Plan 114.4.4.3 V4.2: generate runtime trampolines для fully-const fn
    // used as first-class values. Inserted в module.items BEFORE validate +
    // retain. Trampoline names (`<orig>__trampoline`) survive retain since
    // const_names filter matches только original names.
    let (trampoline_set, t_errors) = crate::const_fn_trampoline::generate_const_fn_trampolines(
        module,
        &owned.fns,
        &owned.aliases,
    );
    errors.extend(t_errors);

    // Plan 114.4.4 Ф.2 (D199 V3): friendly UX validation — detect runtime
    // misuse of const fn names BEFORE codegen drops them. After this pass
    // any Ident referencing const fn в non-call-func position → error
    // с actionable suggestion.
    // Plan 114.4.4.3 V4.2: skip validation для names в trampoline_set —
    // those usages already rewritten к trampoline по step 4.
    validate_const_fn_runtime_uses(module, &owned, &trampoline_set, &mut errors);

    // Drop const fn declarations + alias const decls (V2 Ф.5) из items.
    let const_names = owned.names();
    let alias_names: std::collections::HashSet<String> = owned.aliases.keys().cloned().collect();
    module.items.retain(|it| match it {
        Item::Fn(fd) => !const_names.contains(&fd.name),
        Item::Const(c) => !alias_names.contains(&c.name),
        _ => true,
    });
    for pf in &mut module.peer_files {
        pf.items_here.retain(|it| match it {
            Item::Fn(fd) => !const_names.contains(&fd.name),
            Item::Const(c) => !alias_names.contains(&c.name),
            _ => true,
        });
    }
    errors
}

/// Plan 114.4.4 Ф.3 V3: control flow в const fn body — block execution
/// result. Value(v) — normal completion; Break/Continue — loop control;
/// Unit — block ended без produced value.
enum BlockFlow {
    Value(ConstValue),
    Break,
    Continue,
    Unit,
}

/// Plan 114.4.4 Ф.3 V3: loop iteration limit (anti-infinite-loop guard).
/// Configurable через `#fn_eval_max_iterations(N)` per fn (V4 followup).
const MAX_LOOP_ITERATIONS: usize = 10_000;

/// Self-owning evaluator wrapper — holds `Vec<FnDecl>` cloned from the
/// module, then exposes `&FnDecl` borrows internally. Avoids the
/// borrow-checker conflict of holding `&FnDecl` from `module.items`
/// while mutating other AST nodes.
struct OwnedEvaluator {
    /// Fully-const fn registry — evaluator inlines + drops these.
    fns: HashMap<String, crate::ast::FnDecl>,
    memo: HashMap<(String, Vec<ConstValue>), ConstValue>,
    /// Plan 114.4.3 Ф.3 V2: mixed fn registry — name → Vec<bool> per param
    /// indicating is_const. Used для call-site validation: const-param args
    /// must be constexpr literals. Mixed fns remain в codegen (not dropped).
    mixed_const_params: HashMap<String, Vec<bool>>,
    /// Plan 114.4.4.5 V4.1: mixed fn FnDecl cache — для true monomorphization
    /// (per-const-arg specialization). Populated together с mixed_const_params.
    mixed_fns: HashMap<String, crate::ast::FnDecl>,
    /// Plan 114.4.3 Ф.5 V2: const fn aliases — `const ALIAS = const_fn`.
    /// Walker substitutes Ident("ALIAS") → Ident("ORIGINAL") at call sites.
    /// Alias decls removed из items в retain phase.
    aliases: HashMap<String, String>,
}

impl OwnedEvaluator {
    fn new(decls: Vec<crate::ast::FnDecl>) -> Self {
        let mut fns = HashMap::new();
        for fd in decls {
            fns.insert(fd.name.clone(), fd);
        }
        Self {
            fns,
            memo: HashMap::new(),
            mixed_const_params: HashMap::new(),
            mixed_fns: HashMap::new(),
            aliases: HashMap::new(),
        }
    }
    /// Plan 114.4.3 Ф.3: register mixed const fn — для call-site validation.
    /// Plan 114.4.4.5 V4.1: also stores FnDecl for monomorphization.
    fn register_mixed(&mut self, name: String, const_param_flags: Vec<bool>, fd: crate::ast::FnDecl) {
        self.mixed_const_params.insert(name.clone(), const_param_flags);
        self.mixed_fns.insert(name, fd);
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
        // Plan 114.4.4 Ф.1 (D199 V3): per-fn override depth limit
        // через `#fn_eval_max_depth(N)` attribute.
        let depth_limit = self.fns.get(name)
            .and_then(|fd| fd.fn_eval_max_depth)
            .map(|n| n as usize)
            .unwrap_or(MAX_EVAL_DEPTH);
        if depth >= depth_limit {
            return Err(Diagnostic::new(
                format!(
                    "[E_CONST_FN_EVAL_DEPTH_EXCEEDED] const fn evaluation exceeded \
                     depth limit {} в call to `{}` — likely runaway recursion. \
                     Use `#fn_eval_max_depth(N)` attribute для override \
                     (V3 Plan 114.4.4 Ф.1). D199 V3.",
                    depth_limit, name
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
        match self.exec_block_seq(block, &mut env, current_fn, depth)? {
            BlockFlow::Value(v) => Ok(v),
            BlockFlow::Break | BlockFlow::Continue => Err(Diagnostic::new(
                "[E_CONST_FN_CONTROL_FLOW] break/continue outside loop (D199 V3).".to_string(),
                block.span,
            )),
            BlockFlow::Unit => Err(Diagnostic::new(
                "[E_CONST_FN_EVAL_PANIC] const fn body produced no value (D199).".to_string(),
                block.span,
            )),
        }
    }

    /// Plan 114.4.4 Ф.3 V3: execute block stmts с поддержкой
    /// mut let / assign / break / continue. Returns BlockFlow.
    fn exec_block_seq(
        &mut self,
        block: &Block,
        env: &mut HashMap<String, ConstValue>,
        current_fn: &str,
        depth: usize,
    ) -> Result<BlockFlow, Diagnostic> {
        for st in &block.stmts {
            match st {
                Stmt::Const(cd) => {
                    let v = self.eval_expr(&cd.value, env, current_fn, depth)?;
                    env.insert(cd.name.clone(), v);
                }
                Stmt::Let(ld) => {
                    let v = self.eval_expr(&ld.value, env, current_fn, depth)?;
                    if let crate::ast::Pattern::Ident { name, .. } = &ld.pattern {
                        env.insert(name.clone(), v);
                    }
                }
                Stmt::Assign { target, value, .. } => {
                    let v = self.eval_expr(value, env, current_fn, depth)?;
                    if let ExprKind::Ident(name) = &target.kind {
                        env.insert(name.clone(), v);
                    } else {
                        return Err(Diagnostic::new(
                            "[E_CONST_FN_EVAL_PANIC] non-ident assign target в const \
                             fn body (D199 V3).".to_string(),
                            target.span,
                        ));
                    }
                }
                Stmt::Break(_) => return Ok(BlockFlow::Break),
                Stmt::Continue(_) => return Ok(BlockFlow::Continue),
                Stmt::Expr(e) => {
                    // Plan 114.4.4 Ф.3 V3: control-flow forms требуют
                    // mut env propagation + break/continue propagation.
                    match &e.kind {
                        ExprKind::For { pattern, iter, body, .. } => {
                            self.exec_for_loop(pattern, iter, body, env, current_fn, depth, e.span)?;
                        }
                        ExprKind::While { cond, body, .. } => {
                            self.exec_while_loop(cond, body, env, current_fn, depth, e.span)?;
                        }
                        ExprKind::Loop { body, .. } => {
                            self.exec_loop_loop(body, env, current_fn, depth, e.span)?;
                        }
                        ExprKind::If { cond, then, else_ } => {
                            // If-stmt пропагирует break/continue из branches.
                            let cv = self.eval_expr(cond, env, current_fn, depth)?;
                            let go = match cv {
                                ConstValue::Bool(b) => b,
                                _ => return Err(Diagnostic::new(
                                    "[E_CONST_FN_EVAL_PANIC] if-cond не bool (D199 V3).".to_string(),
                                    cond.span,
                                )),
                            };
                            if go {
                                match self.exec_block_seq(then, env, current_fn, depth)? {
                                    BlockFlow::Break => return Ok(BlockFlow::Break),
                                    BlockFlow::Continue => return Ok(BlockFlow::Continue),
                                    _ => {}
                                }
                            } else if let Some(eb) = else_ {
                                match eb {
                                    crate::ast::ElseBranch::Block(b) => {
                                        match self.exec_block_seq(b, env, current_fn, depth)? {
                                            BlockFlow::Break => return Ok(BlockFlow::Break),
                                            BlockFlow::Continue => return Ok(BlockFlow::Continue),
                                            _ => {}
                                        }
                                    }
                                    crate::ast::ElseBranch::If(_ie) => {
                                        // Else-if branch: re-process через
                                        // synthetic if-expr.
                                        let _ = self.eval_expr(e, env, current_fn, depth)?;
                                    }
                                }
                            }
                        }
                        _ => {
                            let _ = self.eval_expr(e, env, current_fn, depth)?;
                        }
                    }
                }
                Stmt::Return { value: Some(e), .. } => {
                    let v = self.eval_expr(e, env, current_fn, depth)?;
                    return Ok(BlockFlow::Value(v));
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
            let v = self.eval_expr(trail, env, current_fn, depth)?;
            return Ok(BlockFlow::Value(v));
        }
        // Block без trailing value — последний Stmt::Expr может быть value.
        // Если последний — Stmt::Expr, take его value.
        if let Some(Stmt::Expr(e)) = block.stmts.last() {
            let v = self.eval_expr(e, env, current_fn, depth)?;
            return Ok(BlockFlow::Value(v));
        }
        Ok(BlockFlow::Unit)
    }

    /// Plan 114.4.4 Ф.3 V3: for-loop с mut env propagation.
    fn exec_for_loop(
        &mut self,
        pattern: &crate::ast::Pattern,
        iter: &Expr,
        body: &Block,
        env: &mut HashMap<String, ConstValue>,
        current_fn: &str,
        depth: usize,
        span: Span,
    ) -> Result<(), Diagnostic> {
        let (start, end, inclusive) = match &iter.kind {
            ExprKind::Range { start: Some(s), end: Some(e), inclusive } => {
                let sv = self.eval_expr(s, env, current_fn, depth)?;
                let ev_v = self.eval_expr(e, env, current_fn, depth)?;
                let si = match sv {
                    ConstValue::Int(n) => n,
                    _ => return Err(Diagnostic::new(
                        "[E_CONST_FN_EVAL_PANIC] for range start не int (D199 V3).".to_string(),
                        s.span,
                    )),
                };
                let ei = match ev_v {
                    ConstValue::Int(n) => n,
                    _ => return Err(Diagnostic::new(
                        "[E_CONST_FN_EVAL_PANIC] for range end не int (D199 V3).".to_string(),
                        e.span,
                    )),
                };
                (si, ei, *inclusive)
            }
            _ => return Err(Diagnostic::new(
                "[E_CONST_FN_CONTROL_FLOW] for-loop iter в const fn body V3.0 \
                 поддерживает только literal range START..END (D199).".to_string(),
                iter.span,
            )),
        };
        let var_name = match pattern {
            crate::ast::Pattern::Ident { name, .. } => name.clone(),
            crate::ast::Pattern::Wildcard(_) => "_".to_string(),
            _ => return Err(Diagnostic::new(
                "[E_CONST_FN_PATTERN_NOT_SUPPORTED] for-loop pattern (D199 V3).".to_string(),
                iter.span,
            )),
        };
        let end_excl = if inclusive { end + 1 } else { end };
        let mut iter_count = 0usize;
        for i in start..end_excl {
            if iter_count >= MAX_LOOP_ITERATIONS {
                return Err(Diagnostic::new(
                    format!(
                        "[E_CONST_FN_EVAL_ITERATIONS_EXCEEDED] for-loop iterations \
                         exceeded {} (D199 V3).",
                        MAX_LOOP_ITERATIONS
                    ),
                    span,
                ));
            }
            iter_count += 1;
            env.insert(var_name.clone(), ConstValue::Int(i));
            match self.exec_block_seq(body, env, current_fn, depth)? {
                BlockFlow::Break => { env.remove(&var_name); return Ok(()); }
                _ => {}
            }
        }
        env.remove(&var_name);
        Ok(())
    }

    /// Plan 114.4.4 Ф.3 V3: while-loop с mut env propagation.
    fn exec_while_loop(
        &mut self,
        cond: &Expr,
        body: &Block,
        env: &mut HashMap<String, ConstValue>,
        current_fn: &str,
        depth: usize,
        span: Span,
    ) -> Result<(), Diagnostic> {
        let mut iter_count = 0usize;
        loop {
            if iter_count >= MAX_LOOP_ITERATIONS {
                return Err(Diagnostic::new(
                    format!(
                        "[E_CONST_FN_EVAL_ITERATIONS_EXCEEDED] while-loop iterations \
                         exceeded {} (D199 V3).",
                        MAX_LOOP_ITERATIONS
                    ),
                    span,
                ));
            }
            iter_count += 1;
            let cv = self.eval_expr(cond, env, current_fn, depth)?;
            let go = match cv {
                ConstValue::Bool(b) => b,
                _ => return Err(Diagnostic::new(
                    "[E_CONST_FN_EVAL_PANIC] while-cond не bool (D199 V3).".to_string(),
                    cond.span,
                )),
            };
            if !go { break; }
            match self.exec_block_seq(body, env, current_fn, depth)? {
                BlockFlow::Break => break,
                _ => {}
            }
        }
        Ok(())
    }

    /// Plan 114.4.4 Ф.3 V3: loop {} с mut env propagation.
    fn exec_loop_loop(
        &mut self,
        body: &Block,
        env: &mut HashMap<String, ConstValue>,
        current_fn: &str,
        depth: usize,
        span: Span,
    ) -> Result<(), Diagnostic> {
        let mut iter_count = 0usize;
        loop {
            if iter_count >= MAX_LOOP_ITERATIONS {
                return Err(Diagnostic::new(
                    format!(
                        "[E_CONST_FN_EVAL_ITERATIONS_EXCEEDED] loop iterations exceeded \
                         {} — likely missing break (D199 V3).",
                        MAX_LOOP_ITERATIONS
                    ),
                    span,
                ));
            }
            iter_count += 1;
            match self.exec_block_seq(body, env, current_fn, depth)? {
                BlockFlow::Break => break,
                _ => {}
            }
        }
        Ok(())
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
                // Plan 114.4.3 Ф.4 V2: turbofish callee для generic const fn.
                // Plan 114.4.4 Ф.5 V4: extract type_args (для sizeof[T]).
                let (callee_name, type_args) = match &func.kind {
                    ExprKind::Ident(n) => (n.clone(), Vec::new()),
                    ExprKind::TurboFish { base, type_args } => {
                        if let ExprKind::Ident(n) = &base.kind {
                            (n.clone(), type_args.clone())
                        } else {
                            return Err(Diagnostic::new(
                                "[E_CONST_FN_EVAL_PANIC] non-ident turbofish base \
                                 (D199).".to_string(),
                                expr.span,
                            ));
                        }
                    }
                    _ => return Err(Diagnostic::new(
                        "[E_CONST_FN_EVAL_PANIC] non-ident callee (D199).".to_string(),
                        expr.span,
                    )),
                };
                // Plan 114.4.4 Ф.5 V4: t-reflection intrinsics.
                if callee_name == "sizeof" || callee_name == "align_of" {
                    if type_args.len() != 1 {
                        return Err(Diagnostic::new(
                            format!(
                                "[E_CONST_FN_EVAL_PANIC] `{}[T]()` requires exactly 1 type \
                                 argument (D199 V4).",
                                callee_name
                            ),
                            expr.span,
                        ));
                    }
                    let size = type_size_or_align(&type_args[0], callee_name == "align_of")
                        .ok_or_else(|| Diagnostic::new(
                            format!(
                                "[E_CONST_FN_GENERIC_NEEDS_T_REFLECTION] `{}[T]()` для \
                                 type {} not supported в V4.0 — only primitives \
                                 (int/i*/u*/f*/bool/char) supported. Record/generic \
                                 type reflection — followup `[M-114.4.4-record-reflection]`.",
                                callee_name, simple_type_name_str(&type_args[0])
                                    .unwrap_or_else(|| "<complex>".to_string())
                            ),
                            expr.span,
                        ))?;
                    return Ok(ConstValue::Int(size));
                }
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
                // Plan 114.4.4 Ф.4 V4: distinguish const fn vs variant constructor.
                let resolved_name = self.aliases.get(&callee_name).cloned().unwrap_or(callee_name.clone());
                if !self.fns.contains_key(&resolved_name)
                    && callee_name.chars().next().map_or(false, |c| c.is_uppercase())
                {
                    return Ok(ConstValue::Variant(callee_name, arg_vals));
                }
                self.eval_call_inner(&resolved_name, arg_vals, expr.span, depth)
            }
            // Plan 114.4.4 Ф.4 V4: tuple literal → ConstValue::Tuple.
            ExprKind::TupleLit(elems) => {
                let mut vs = Vec::with_capacity(elems.len());
                for e in elems {
                    vs.push(self.eval_expr(e, env, current_fn, depth)?);
                }
                Ok(ConstValue::Tuple(vs))
            }
            // Plan 114.4.4 Ф.4 V4: record literal → ConstValue::Record.
            ExprKind::RecordLit { fields, .. } => {
                let mut vs = Vec::with_capacity(fields.len());
                for f in fields {
                    if f.is_spread {
                        return Err(Diagnostic::new(
                            "[E_CONST_FN_EVAL_PANIC] spread в record literal (D199).".to_string(),
                            expr.span,
                        ));
                    }
                    let v = if let Some(val) = &f.value {
                        self.eval_expr(val, env, current_fn, depth)?
                    } else {
                        // Shorthand `{ name }` — resolve via env lookup.
                        env.get(&f.name).cloned().ok_or_else(|| Diagnostic::new(
                            format!("[E_CONST_FN_EVAL_PANIC] record shorthand `{}` not bound (D199).", f.name),
                            f.span,
                        ))?
                    };
                    vs.push((f.name.clone(), v));
                }
                Ok(ConstValue::Record(vs))
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
                        // Plan 114.4.4 Ф.3 V3: if без else allowed
                        // (side-effect statement в loops).
                        None => Ok(ConstValue::Unit),
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
            // Plan 114.4.4 Ф.3 V3: For loop — only literal Range iter в V3.0.
            ExprKind::For { pattern, iter, body, .. } => {
                let (start, end, inclusive) = match &iter.kind {
                    ExprKind::Range { start: Some(s), end: Some(e), inclusive } => {
                        let sv = self.eval_expr(s, env, current_fn, depth)?;
                        let ev_v = self.eval_expr(e, env, current_fn, depth)?;
                        let si = match sv {
                            ConstValue::Int(n) => n,
                            _ => return Err(Diagnostic::new(
                                "[E_CONST_FN_EVAL_PANIC] for range start не int (D199 V3).".to_string(),
                                s.span,
                            )),
                        };
                        let ei = match ev_v {
                            ConstValue::Int(n) => n,
                            _ => return Err(Diagnostic::new(
                                "[E_CONST_FN_EVAL_PANIC] for range end не int (D199 V3).".to_string(),
                                e.span,
                            )),
                        };
                        (si, ei, *inclusive)
                    }
                    _ => return Err(Diagnostic::new(
                        "[E_CONST_FN_CONTROL_FLOW] for-loop iter в const fn body V3.0 \
                         поддерживает только literal range `START..END` или `START..=END` \
                         (D199). Followup `[M-114.4.4-for-iter-array]` для array iter.".to_string(),
                        iter.span,
                    )),
                };
                let var_name = match pattern {
                    crate::ast::Pattern::Ident { name, .. } => name.clone(),
                    crate::ast::Pattern::Wildcard(_) => "_".to_string(),
                    _ => return Err(Diagnostic::new(
                        "[E_CONST_FN_PATTERN_NOT_SUPPORTED] for-loop pattern должен быть \
                         single ident или wildcard (D199 V3).".to_string(),
                        iter.span,
                    )),
                };
                let mut env_l = env.clone();
                let end_excl = if inclusive { end + 1 } else { end };
                let mut iter_count = 0usize;
                for i in start..end_excl {
                    if iter_count >= MAX_LOOP_ITERATIONS {
                        return Err(Diagnostic::new(
                            format!(
                                "[E_CONST_FN_EVAL_ITERATIONS_EXCEEDED] for-loop iterations \
                                 exceeded {} (D199 V3). Use smaller range или file \
                                 followup `[M-114.4.4-configurable-iterations]`.",
                                MAX_LOOP_ITERATIONS
                            ),
                            expr.span,
                        ));
                    }
                    iter_count += 1;
                    env_l.insert(var_name.clone(), ConstValue::Int(i));
                    match self.exec_block_seq(body, &mut env_l, current_fn, depth)? {
                        BlockFlow::Break => break,
                        BlockFlow::Continue | BlockFlow::Value(_) | BlockFlow::Unit => {}
                    }
                }
                // For-loop само по себе returns Unit. Caller (block stmt list)
                // должен handle.
                Ok(ConstValue::Unit)
            }
            // Plan 114.4.4 Ф.3 V3: While loop.
            ExprKind::While { cond, body, .. } => {
                let mut env_l = env.clone();
                let mut iter_count = 0usize;
                loop {
                    if iter_count >= MAX_LOOP_ITERATIONS {
                        return Err(Diagnostic::new(
                            format!(
                                "[E_CONST_FN_EVAL_ITERATIONS_EXCEEDED] while-loop iterations \
                                 exceeded {} (D199 V3).",
                                MAX_LOOP_ITERATIONS
                            ),
                            expr.span,
                        ));
                    }
                    iter_count += 1;
                    let cv = self.eval_expr(cond, &env_l, current_fn, depth)?;
                    let go = match cv {
                        ConstValue::Bool(b) => b,
                        _ => return Err(Diagnostic::new(
                            "[E_CONST_FN_EVAL_PANIC] while-cond не bool (D199 V3).".to_string(),
                            cond.span,
                        )),
                    };
                    if !go { break; }
                    match self.exec_block_seq(body, &mut env_l, current_fn, depth)? {
                        BlockFlow::Break => break,
                        BlockFlow::Continue | BlockFlow::Value(_) | BlockFlow::Unit => {}
                    }
                }
                Ok(ConstValue::Unit)
            }
            // Plan 114.4.4 Ф.3 V3: Loop — must break to exit.
            ExprKind::Loop { body, .. } => {
                let mut env_l = env.clone();
                let mut iter_count = 0usize;
                loop {
                    if iter_count >= MAX_LOOP_ITERATIONS {
                        return Err(Diagnostic::new(
                            format!(
                                "[E_CONST_FN_EVAL_ITERATIONS_EXCEEDED] loop iterations \
                                 exceeded {} — likely missing break (D199 V3).",
                                MAX_LOOP_ITERATIONS
                            ),
                            expr.span,
                        ));
                    }
                    iter_count += 1;
                    match self.exec_block_seq(body, &mut env_l, current_fn, depth)? {
                        BlockFlow::Break => break,
                        BlockFlow::Continue | BlockFlow::Value(_) | BlockFlow::Unit => {}
                    }
                }
                Ok(ConstValue::Unit)
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
            // Plan 114.4.2/.4.3 V2: fully-const fn bodies will be dropped — skip.
            // Mixed fns остаются runtime — walk их body для const-fn-call
            // replacement и default param rewrite.
            let all_const_params = !fd.params.is_empty()
                && fd.params.iter().all(|p| p.is_const);
            let is_fully_const = (all_const_params || fd.params.is_empty())
                && fd.return_is_const;
            if is_fully_const {
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
    // Plan 114.4.3 Ф.3 V2: mixed const fn call-site validation.
    // Mixed fns остаются в codegen, не evaluated, но const params на call site
    // требуют constexpr arg.
    if let ExprKind::Call { func, args, trailing: None } = &e.kind {
        // Plan 114.4.4 Ф.5 V4: sizeof[T]() / align_of[T]() intrinsics —
        // replace call с literal Int.
        if let ExprKind::TurboFish { base, type_args } = &func.kind {
            if let ExprKind::Ident(n) = &base.kind {
                if (n == "sizeof" || n == "align_of") && args.is_empty() && type_args.len() == 1 {
                    match type_size_or_align(&type_args[0], n == "align_of") {
                        Some(size) => {
                            let span = e.span;
                            *e = Expr { kind: ExprKind::IntLit(size), span };
                            return;
                        }
                        None => {
                            errors.push(Diagnostic::new(
                                format!(
                                    "[E_CONST_FN_GENERIC_NEEDS_T_REFLECTION] `{}[T]()` для \
                                     type {} not supported в V4.0 (D199). Only primitives. \
                                     Followup `[M-114.4.4-record-reflection]`.",
                                    n,
                                    simple_type_name_str(&type_args[0])
                                        .unwrap_or_else(|| "<complex>".to_string())
                                ),
                                e.span,
                            ));
                            return;
                        }
                    }
                }
            }
        }
        // Plan 114.4.3 Ф.4 V2: turbofish callee — unwrap base Ident.
        let raw_name_opt: Option<String> = match &func.kind {
            ExprKind::Ident(n) => Some(n.clone()),
            ExprKind::TurboFish { base, .. } => {
                if let ExprKind::Ident(n) = &base.kind {
                    Some(n.clone())
                } else { None }
            }
            _ => None,
        };
        if let Some(raw_name) = raw_name_opt {
            // Plan 114.4.3 Ф.5 V2: alias resolution. Если callee — alias,
            // redirect к original const fn name.
            let resolved_name = ev.aliases.get(&raw_name).cloned().unwrap_or(raw_name);
            let name = &resolved_name;
            if let Some(flags) = ev.mixed_const_params.get(name).cloned() {
                for (idx, (a, is_const)) in args.iter().zip(flags.iter()).enumerate() {
                    if !is_const { continue; }
                    let ok = match a {
                        crate::ast::CallArg::Item(ae) => try_literal_to_value(ae).is_some(),
                        _ => false,
                    };
                    if !ok {
                        errors.push(Diagnostic::new(
                            format!(
                                "[E_CONST_FN_NON_CONST_ARG] call to mixed const fn `{}`: \
                                 argument {} corresponds to `const` parameter — must be \
                                 constexpr literal (D199 V2). Replace runtime expression \
                                 с literal value либо помарковать parameter без `const`.",
                                name, idx + 1
                            ),
                            e.span,
                        ));
                        break;
                    }
                }
                // Mixed fns остаются в codegen — не evaluate / replace.
                return;
            }
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

// =========================================================================
// Plan 114.4.4 Ф.2 (D199 V3): friendly UX validation — detect runtime
// misuse of const fn name + emit actionable diagnostics.
// =========================================================================

/// Walks module after rewriter to find runtime contexts referencing
/// const fn names (in positions where they can't be used due to
/// codegen drop). Emits:
/// - `E_CONST_FN_FIRST_CLASS` для runtime let-bindings (`ro f = const_fn`).
/// - `E_CONST_FN_FIRST_CLASS_RUNTIME_HOF` для Call args referencing
///   const fn в not-callee positions (`map(arr, const_fn)`).
fn validate_const_fn_runtime_uses(
    module: &crate::ast::Module,
    ev: &OwnedEvaluator,
    trampoline_set: &std::collections::HashSet<String>,
    errors: &mut Vec<Diagnostic>,
) {
    // Plan 114.4.4.3 V4.2: names в trampoline_set теперь имеют runtime
    // symbol `<name>__trampoline` и first-class uses переписаны на него
    // step'ом 4 trampoline pass. Не флагаем как ошибку.
    // Aliases для names в trampoline_set тоже skip — они resolve'ятся.
    let cf_names: std::collections::HashSet<String> = ev.fns.keys()
        .chain(ev.mixed_const_params.keys())
        .chain(ev.aliases.keys())
        .filter(|name| {
            if trampoline_set.contains(name.as_str()) { return false; }
            // Check alias target.
            if let Some(target) = ev.aliases.get(name.as_str()) {
                if trampoline_set.contains(target.as_str()) { return false; }
            }
            true
        })
        .cloned()
        .collect();
    if cf_names.is_empty() {
        return;
    }
    let mut ctx = ValidateCtx { cf_names: &cf_names, errors };
    for item in &module.items {
        ctx.visit_item(item);
    }
    for pf in &module.peer_files {
        for item in &pf.items_here {
            ctx.visit_item(item);
        }
    }
}

struct ValidateCtx<'a, 'b> {
    cf_names: &'a std::collections::HashSet<String>,
    errors: &'b mut Vec<Diagnostic>,
}

impl<'a, 'b> ValidateCtx<'a, 'b> {
    fn visit_item(&mut self, item: &crate::ast::Item) {
        use crate::ast::Item;
        match item {
            Item::Fn(fd) => {
                let all_const_params = !fd.params.is_empty()
                    && fd.params.iter().all(|p| p.is_const);
                let is_fully_const = (all_const_params || fd.params.is_empty())
                    && fd.return_is_const;
                if is_fully_const {
                    // body будет dropped — skip validation внутри fully-const.
                    return;
                }
                match &fd.body {
                    FnBody::Expr(e) => self.visit_expr(e),
                    FnBody::Block(b) => self.visit_block(b),
                    FnBody::External => {}
                }
            }
            Item::Const(_) => { /* alias const RHS already validated via rewriter */ }
            Item::Let(l) => {
                self.check_runtime_let_to_const_fn(&l.value, l.value.span);
                self.visit_expr(&l.value);
            }
            Item::Test(t) => self.visit_block(&t.body),
            Item::Bench(b) => {
                for s in &b.setup { self.visit_stmt(s); }
                self.visit_block(&b.measure_body);
                for s in &b.teardown { self.visit_stmt(s); }
            }
            Item::Type(_) | Item::Lemma(_) => {}
        }
    }
    fn visit_block(&mut self, b: &crate::ast::Block) {
        for s in &b.stmts { self.visit_stmt(s); }
        if let Some(t) = &b.trailing { self.visit_expr(t); }
    }
    fn visit_stmt(&mut self, s: &crate::ast::Stmt) {
        use crate::ast::Stmt;
        match s {
            Stmt::Let(d) => {
                self.check_runtime_let_to_const_fn(&d.value, d.value.span);
                self.visit_expr(&d.value);
            }
            Stmt::Const(d) => {
                // const ALIAS = const_fn — handled by rewriter alias map.
                // Don't recurse — Ident на const fn в const RHS is OK.
            }
            Stmt::Expr(e) => self.visit_expr(e),
            Stmt::Assign { target, value, .. } => {
                self.visit_expr(target);
                self.visit_expr(value);
            }
            Stmt::Return { value, .. } => {
                if let Some(v) = value { self.visit_expr(v); }
            }
            Stmt::Throw { value, .. } => self.visit_expr(value),
            Stmt::Break(_) | Stmt::Continue(_) => {}
            Stmt::Defer { body, .. } | Stmt::ErrDefer { body, .. }
            | Stmt::OkDefer { body, .. } | Stmt::DeferWithResult { body, .. } => {
                self.visit_expr(body);
            }
            Stmt::ConsumeScope { init, body, .. } => {
                self.visit_expr(init);
                self.visit_block(body);
            }
            Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => {
                self.visit_expr(expr);
            }
            Stmt::Apply { .. } | Stmt::Calc { .. } | Stmt::Reveal { .. } => {}
        }
    }
    /// Check if `value` is a bare Ident referring to const fn — emit
    /// `E_CONST_FN_FIRST_CLASS` если так.
    fn check_runtime_let_to_const_fn(&mut self, value: &Expr, span: Span) {
        if let ExprKind::Ident(name) = &value.kind {
            if self.cf_names.contains(name) {
                self.errors.push(Diagnostic::new(
                    format!(
                        "[E_CONST_FN_FIRST_CLASS] runtime binding к const fn `{}` \
                         не разрешено (D199 V3). const fn не имеет runtime symbol — \
                         dropped из codegen. Workarounds:\n  \
                         (1) `const ALIAS = {}` — compile-time alias (если используется \
                         только в const RHS).\n  \
                         (2) `ro f = |x, ...| {}(x, ...)` — runtime lambda wrapping \
                         const fn call с literal args.\n  \
                         Followup `[M-114.4.3-runtime-hof]` для real first-class \
                         через runtime trampoline.",
                        name, name, name
                    ),
                    span,
                ));
            }
        }
    }
    fn visit_expr(&mut self, e: &Expr) {
        use crate::ast::CallArg;
        match &e.kind {
            ExprKind::Call { func, args, trailing } => {
                // func может быть Ident к const fn — это OK (call site).
                // Аргументы — если Ident к const fn → HOF misuse.
                self.visit_expr(func);
                for a in args {
                    match a {
                        CallArg::Item(ae) | CallArg::Spread(ae) => {
                            self.check_runtime_hof_use(ae);
                            self.visit_expr(ae);
                        }
                        CallArg::Named { value, .. } => {
                            self.check_runtime_hof_use(value);
                            self.visit_expr(value);
                        }
                    }
                }
                if let Some(t) = trailing {
                    match t {
                        crate::ast::Trailing::Block(b) => self.visit_block(b),
                        crate::ast::Trailing::Fn(sb) => self.visit_fn_body(&sb.body),
                        crate::ast::Trailing::LegacyBlockWithParams(tb) => self.visit_block(&tb.body),
                    }
                }
            }
            _ => self.recurse_children(e),
        }
    }
    /// If arg is bare Ident referring const fn → HOF misuse error.
    fn check_runtime_hof_use(&mut self, e: &Expr) {
        if let ExprKind::Ident(name) = &e.kind {
            if self.cf_names.contains(name) {
                self.errors.push(Diagnostic::new(
                    format!(
                        "[E_CONST_FN_FIRST_CLASS_RUNTIME_HOF] passing const fn `{}` \
                         to runtime higher-order function (D199 V3). const fn не имеет \
                         runtime symbol — dropped из codegen. Wrap в lambda:\n  \
                         `... |args| {}(args) ...`\n  \
                         Followup `[M-114.4.3-runtime-hof]` для automatic trampoline.",
                        name, name
                    ),
                    e.span,
                ));
            }
        }
    }
    fn visit_fn_body(&mut self, body: &FnBody) {
        match body {
            FnBody::Expr(e) => self.visit_expr(e),
            FnBody::Block(b) => self.visit_block(b),
            FnBody::External => {}
        }
    }
    fn recurse_children(&mut self, e: &Expr) {
        use crate::ast::{ArrayElem, ElseBranch, MapElem};
        match &e.kind {
            ExprKind::IntLit(_) | ExprKind::FloatLit(_) | ExprKind::StrLit(_)
            | ExprKind::BoolLit(_) | ExprKind::CharLit(_) | ExprKind::UnitLit
            | ExprKind::Ident(_) | ExprKind::Path(_) | ExprKind::SelfAccess => {}
            ExprKind::InterpolatedStr { parts } => {
                for p in parts {
                    if let crate::ast::InterpStrPart::Expr(ex) = p { self.visit_expr(ex); }
                }
            }
            ExprKind::ArrayLit(elems) => {
                for el in elems {
                    match el {
                        ArrayElem::Item(x) | ArrayElem::Spread(x) => {
                            // Array literal arg — same as HOF concern.
                            self.check_runtime_hof_use(x);
                            self.visit_expr(x);
                        }
                    }
                }
            }
            ExprKind::MapLit { elems, .. } => {
                for el in elems {
                    match el {
                        MapElem::Pair(k, v) => { self.visit_expr(k); self.visit_expr(v); }
                        MapElem::Spread(s) => self.visit_expr(s),
                    }
                }
            }
            ExprKind::RecordLit { fields, .. } => {
                for f in fields {
                    if let Some(v) = &f.value { self.visit_expr(v); }
                }
            }
            ExprKind::TupleLit(items) => for x in items { self.visit_expr(x); },
            ExprKind::Member { obj, .. } => self.visit_expr(obj),
            ExprKind::Index { obj, index } => { self.visit_expr(obj); self.visit_expr(index); }
            ExprKind::TurboFish { base, .. } => self.visit_expr(base),
            ExprKind::Call { .. } => {} // handled в visit_expr Call arm
            ExprKind::Try(x) | ExprKind::Bang(x) => self.visit_expr(x),
            ExprKind::Coalesce(a, b) => { self.visit_expr(a); self.visit_expr(b); }
            ExprKind::As(x, _) | ExprKind::Is(x, _) => self.visit_expr(x),
            ExprKind::Binary { left, right, .. } => {
                self.visit_expr(left); self.visit_expr(right);
            }
            ExprKind::Unary { operand, .. } => self.visit_expr(operand),
            ExprKind::If { cond, then, else_ } => {
                self.visit_expr(cond);
                self.visit_block(then);
                if let Some(eb) = else_ {
                    match eb {
                        ElseBranch::Block(b) => self.visit_block(b),
                        ElseBranch::If(ie) => self.visit_expr(ie),
                    }
                }
            }
            ExprKind::IfLet { scrutinee, then, else_, .. } => {
                self.visit_expr(scrutinee);
                self.visit_block(then);
                if let Some(eb) = else_ {
                    match eb {
                        ElseBranch::Block(b) => self.visit_block(b),
                        ElseBranch::If(ie) => self.visit_expr(ie),
                    }
                }
            }
            ExprKind::Match { scrutinee, arms } => {
                self.visit_expr(scrutinee);
                for arm in arms {
                    if let Some(g) = &arm.guard { self.visit_expr(g); }
                    match &arm.body {
                        crate::ast::MatchArmBody::Expr(e) => self.visit_expr(e),
                        crate::ast::MatchArmBody::Block(b) => self.visit_block(b),
                    }
                }
            }
            ExprKind::For { iter, body, .. } => { self.visit_expr(iter); self.visit_block(body); }
            ExprKind::ParallelFor { iter, body, .. } => { self.visit_expr(iter); self.visit_block(body); }
            ExprKind::While { cond, body, .. } => { self.visit_expr(cond); self.visit_block(body); }
            ExprKind::WhileLet { scrutinee, body, .. } => { self.visit_expr(scrutinee); self.visit_block(body); }
            ExprKind::Loop { body, .. } => self.visit_block(body),
            ExprKind::Block(b) => self.visit_block(b),
            ExprKind::Lambda { body, .. } => self.visit_expr(body),
            ExprKind::ClosureLight { body, .. } => {
                match body {
                    crate::ast::ClosureBody::Expr(e) => self.visit_expr(e),
                    crate::ast::ClosureBody::Block(b) => self.visit_block(b),
                }
            }
            ExprKind::ClosureFull(sb) => self.visit_fn_body(&sb.body),
            ExprKind::Spawn(b) => self.visit_expr(b),
            ExprKind::Supervised { body, .. } => self.visit_block(body),
            ExprKind::Detach(b) | ExprKind::Blocking(b) => self.visit_block(b),
            ExprKind::With { body, .. } => self.visit_block(body),
            ExprKind::Forbid { body, .. } => self.visit_block(body),
            ExprKind::Realtime { body, .. } => self.visit_block(body),
            ExprKind::Interrupt(opt) => { if let Some(x) = opt { self.visit_expr(x); } }
            ExprKind::HandlerLit { .. } | ExprKind::ProtocolLit { .. } => {}
            _ => {}
        }
    }
}
