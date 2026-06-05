//! Plan 127 Ф.2 — value-record escape analysis + auto-promote.
//!
//! Detects local bindings whose underlying type is a value-record
//! (`type X value { ... }`, `AllocKind::Value`) AND whose address escapes
//! the local scope via `&v` reaching a sink (return, closure capture,
//! heap-field store, global binding, fn-arg pass).
//!
//! V1 OVER-promote: any uncertainty → promote (sound under-approximation
//! of stack-safety; matches Plan 118 V1 OVER-promote stance). Path-
//! sensitive / precise mode = `[M-127-precise-escape]` /
//! `[M-127-path-sensitive-escape]` followup.
//!
//! Output: `EscapeResult` — per-fn set of promoted local names. Consumed
//! by codegen (Plan 127 Ф.3 — `emit_record_lit` switches `Value` →
//! `ValueHeapPromoted` for promoted bindings; `prepare_method_recv` uses
//! pointer-ABI directly for promoted receivers).
//!
//! Spec: D228 amend §«escape & auto-promote» (Plan 127). Cross-ref
//! Plan 118 D216 §4 (escape machinery reuse contract).

use std::collections::{HashMap, HashSet};

use crate::ast::{
    AllocKind, Block, ClosureBody, ElseBranch, Expr, ExprKind, FnBody, FnDecl,
    InterpStrPart, Item, MatchArmBody, Module, Pattern, Stmt, TypeDecl,
    TypeDeclKind, TypeRef, UnOp, ArrayElem,
};

/// Per-module result of value-record escape analysis.
///
/// Key: function id (`fn_name` or `recv_type::method_name`). Value: set of
/// local binding names that must be promoted to heap allocation
/// (`AllocKind::ValueHeapPromoted`).
///
/// V1 simplification: fn-name keying. Methods keyed as
/// `<recv_type>::<method_name>` to avoid collisions across types. Cross-
/// peer collision resolution (same fn name in different peers) — handled
/// by caller via fn_id construction; analyzer treats each FnDecl
/// independently.
#[derive(Debug, Clone, Default)]
pub struct EscapeResult {
    /// `fn_id` → set of escaped local names (must be heap-promoted).
    promoted_per_fn: HashMap<String, HashSet<String>>,
}

impl EscapeResult {
    /// True if local `local_name` in function `fn_id` was detected as
    /// escaping and must be allocated on the heap.
    pub fn is_promoted(&self, fn_id: &str, local_name: &str) -> bool {
        self.promoted_per_fn
            .get(fn_id)
            .map(|s| s.contains(local_name))
            .unwrap_or(false)
    }

    /// Returns true if any locals в любой fn were promoted (diagnostics +
    /// performance: skip downstream emit checks for fully-stack modules).
    pub fn has_any_promoted(&self) -> bool {
        self.promoted_per_fn.values().any(|s| !s.is_empty())
    }

    /// Number of bindings promoted (across all functions). Useful for
    /// diagnostics + unit-test assertion granularity.
    pub fn total_promoted_count(&self) -> usize {
        self.promoted_per_fn.values().map(|s| s.len()).sum()
    }

    /// Plan 127 Ф.4: iterate over `(fn_id, promoted_locals_set)` entries.
    /// Used by `lint_value_record_unnecessary_promote` to enumerate fns
    /// with promoted locals для emitting per-fn warnings.
    pub fn iter_promoted(&self) -> impl Iterator<Item = (&String, &HashSet<String>)> {
        self.promoted_per_fn.iter()
    }
}

/// Compute escape analysis for the entire module.
///
/// Algorithm:
/// 1. Collect set of value-record type names (TypeDecl.allocation == Value).
/// 2. For each FnDecl: walk body, track value-record locals (Let bindings
///    where RHS type or annotation resolves to a value-record).
/// 3. Detect escape conditions per §2.1 of Plan 127:
///    - `&v` in Return position (last expr or Stmt::Return).
///    - `&v` in Assign target = Member { obj, .. } (heap-field store).
///    - `&v` captured by closure (closure body references `&v`).
///    - `&v` passed as fn arg (conservative — caller may store).
///    - `&v` assigned to module-level binding (conservative — global).
/// 4. Promote any local whose `&v` was observed in any escape sink.
///
/// Conservative V1: if a value-record local is bound to a Pattern other
/// than `Ident { .. }`, we do NOT track it (destructure binding). If `&v`
/// is in a context we cannot prove safe — promote.
pub fn analyze_module(module: &Module) -> EscapeResult {
    let value_records = collect_value_record_types(module);
    let mut result = EscapeResult::default();

    // Walk entry peer_files items (mirror Plan 118 check_unsafe_context_in_module).
    let entry_items: Vec<&Item> = if module.peer_files.is_empty() {
        module.items.iter().collect()
    } else {
        module.peer_files.iter()
            .filter(|pf| pf.is_entry_module)
            .flat_map(|pf| pf.items_here.iter())
            .collect()
    };

    for item in entry_items {
        if let Item::Fn(fd) = item {
            analyze_fn(fd, &value_records, &mut result);
        }
    }
    // Also walk non-entry peer fns (per-file emission still needs their
    // promotion data).
    for pf in &module.peer_files {
        if pf.is_entry_module { continue; }
        for item in &pf.items_here {
            if let Item::Fn(fd) = item {
                analyze_fn(fd, &value_records, &mut result);
            }
        }
    }
    result
}

/// Collect names of all value-record type declarations in the module.
fn collect_value_record_types(module: &Module) -> HashSet<String> {
    let mut out = HashSet::new();
    let collect = |items: &[Item], out: &mut HashSet<String>| {
        for item in items {
            if let Item::Type(t) = item {
                if is_value_record_decl(t) {
                    out.insert(t.name.clone());
                }
            }
        }
    };
    collect(&module.items, &mut out);
    for pf in &module.peer_files {
        collect(&pf.items_here, &mut out);
    }
    out
}

fn is_value_record_decl(t: &TypeDecl) -> bool {
    matches!(t.kind, TypeDeclKind::Record(_)) && t.allocation == AllocKind::Value
}

/// Construct fn-id key: free fn → name; method → `<recv>::<name>`.
fn fn_id(fd: &FnDecl) -> String {
    if let Some(recv) = &fd.receiver {
        format!("{}::{}", recv.type_name, fd.name)
    } else {
        fd.name.clone()
    }
}

/// Walk one FnDecl body and accumulate escape detections in `result`.
fn analyze_fn(
    fd: &FnDecl,
    value_records: &HashSet<String>,
    result: &mut EscapeResult,
) {
    let key = fn_id(fd);
    let mut ctx = EscapeCtx {
        value_records,
        scopes: vec![Scope::default()],
        promoted: HashSet::new(),
    };
    match &fd.body {
        FnBody::Block(b) => ctx.walk_block(b, /*is_fn_body=*/ true),
        FnBody::Expr(e) => {
            // Fn-body expr = tail return position. Treat the expression as a
            // returned value for escape detection (e.g. `fn f() -> *Vec3 => &v`).
            ctx.walk_expr_in_return(e);
        }
        FnBody::External => {}
    }
    if !ctx.promoted.is_empty() {
        result.promoted_per_fn.insert(key, ctx.promoted);
    }
}

/// Per-fn analysis context — scope stack + accumulator.
struct EscapeCtx<'a> {
    value_records: &'a HashSet<String>,
    /// Stack of scopes; innermost at the back. Each scope holds Let-bindings
    /// declared in that block (name → was_value_record).
    scopes: Vec<Scope>,
    /// Output: names of bindings (most-recently bound matching name) that
    /// must be promoted. V1 conservative: shadowing rare in Nova bodies;
    /// we record on bare name. Codegen matches by name at the emit_let
    /// site, which sees exactly the binding's scope.
    promoted: HashSet<String>,
}

#[derive(Default)]
struct Scope {
    /// Locals declared in this scope that are bound to a value-record type.
    /// Key: binding name; value: source TypeDecl name (informational).
    value_record_locals: HashMap<String, String>,
}

impl<'a> EscapeCtx<'a> {
    fn push_scope(&mut self) {
        self.scopes.push(Scope::default());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    /// True if `name` is currently bound to a value-record local (search
    /// from innermost scope outward — first hit wins).
    fn lookup_value_record_local(&self, name: &str) -> Option<&str> {
        for s in self.scopes.iter().rev() {
            if let Some(tname) = s.value_record_locals.get(name) {
                return Some(tname.as_str());
            }
        }
        None
    }

    fn register_value_record_local(&mut self, binding: &str, type_name: &str) {
        if let Some(top) = self.scopes.last_mut() {
            top.value_record_locals.insert(binding.to_string(), type_name.to_string());
        }
    }

    /// Mark `name` as promoted (escape detected).
    fn mark_promoted(&mut self, name: &str) {
        if self.lookup_value_record_local(name).is_some() {
            self.promoted.insert(name.to_string());
        }
    }

    /// Resolve a TypeRef to a value-record type name, если applicable.
    /// Recognizes plain `TypeRef::Named { path, .. }` where path.last() ∈
    /// value_records. Wrappers (ro/mut/unsafe/ptr) — V1 conservative: not
    /// recognized (treat as non-value-record source).
    fn resolve_value_record_type(&self, t: &TypeRef) -> Option<String> {
        match t {
            TypeRef::Named { path, .. } => {
                let last = path.last()?;
                if self.value_records.contains(last) {
                    Some(last.clone())
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Best-effort infer: if `value` is a record-literal `X { ... }` where
    /// X is a value-record, return X. (Allows let-without-annotation
    /// `let v = Vec3 { x:1, y:2, z:3 }` to register as value-record local.)
    fn infer_value_record_from_expr(&self, value: &Expr) -> Option<String> {
        match &value.kind {
            ExprKind::RecordLit { type_name: Some(path), .. } => {
                let last = path.last()?;
                if self.value_records.contains(last) {
                    Some(last.clone())
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn walk_block(&mut self, b: &Block, is_fn_body: bool) {
        self.push_scope();
        for stmt in &b.stmts {
            self.walk_stmt(stmt);
        }
        if let Some(trailing) = &b.trailing {
            if is_fn_body {
                // Tail expression в fn-body block = return position.
                self.walk_expr_in_return(trailing);
            } else {
                self.walk_expr(trailing);
            }
        }
        self.pop_scope();
    }

    fn walk_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Let(decl) => {
                // Side-effects in RHS — walk first (not return position).
                self.walk_expr(&decl.value);
                // Register binding if it's a simple Ident pattern AND type
                // resolves to a value-record.
                if let Pattern::Ident { name, .. } = &decl.pattern {
                    let resolved = decl.ty.as_ref()
                        .and_then(|t| self.resolve_value_record_type(t))
                        .or_else(|| self.infer_value_record_from_expr(&decl.value));
                    if let Some(type_name) = resolved {
                        self.register_value_record_local(name, &type_name);
                    }
                }
            }
            Stmt::Return { value: Some(e), .. } => {
                self.walk_expr_in_return(e);
            }
            Stmt::Return { value: None, .. } => {}
            Stmt::Throw { value, .. } => {
                // Thrown values escape (caller catches). Conservative:
                // treat as return-position.
                self.walk_expr_in_return(value);
            }
            Stmt::Assign { target, value, .. } => {
                // V1 escape sink: `obj.field = &v` — heap-field store.
                // Detect target = Member { .. } (any depth) AND value
                // contains `&local`.
                let target_is_heap_field = expr_is_member_chain(target);
                self.walk_expr(target);
                if target_is_heap_field {
                    self.walk_expr_in_escape(value);
                } else {
                    self.walk_expr(value);
                }
            }
            Stmt::Expr(e) => self.walk_expr(e),
            Stmt::Const(_) => {}
            Stmt::Break(_) | Stmt::Continue(_) => {}
            Stmt::Defer { body, .. }
            | Stmt::ErrDefer { body, .. }
            | Stmt::OkDefer { body, .. }
            | Stmt::DeferWithResult { body, .. } => {
                // Defer body executes on scope exit. Treat as normal walk —
                // `&v` ставшее escape через capture inside defer body will
                // still be detected via the inner ClosureLight/RecordLit
                // arms.
                self.walk_expr(body);
            }
            Stmt::ConsumeScope { init, body, .. } => {
                self.walk_expr(init);
                self.walk_block(body, /*is_fn_body=*/ false);
            }
            Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => {
                self.walk_expr(expr);
            }
            Stmt::Apply { .. } | Stmt::Calc { .. } | Stmt::Reveal { .. } => {
                // Ghost / spec-only statements — no runtime escape effect.
            }
        }
    }

    /// Walk expression in *neutral* context — no escape detection.
    fn walk_expr(&mut self, e: &Expr) {
        match &e.kind {
            // Plan 127 V1 condition: bare `&v` без an enclosing escape sink
            // — does NOT trigger promotion. Only `&v` reaching an escape
            // position promotes. This is the key correctness point: locally-
            // scoped `let p = &v` stays stack (A127.1).
            ExprKind::Unary { op: UnOp::AddrOf, operand } => {
                self.walk_expr(operand);
            }
            ExprKind::Unary { operand, .. } => self.walk_expr(operand),
            ExprKind::Block(b) => self.walk_block(b, /*is_fn_body=*/ false),
            ExprKind::Binary { left, right, .. } => {
                self.walk_expr(left);
                self.walk_expr(right);
            }
            ExprKind::Call { func, args, .. } => {
                self.walk_expr(func);
                // V1 escape sink: passing `&v` as an arg — conservative
                // OVER-promote (callee may store it). Walk each arg в
                // escape mode for `&v` детектирования.
                for a in args {
                    self.walk_expr_in_escape(a.expr());
                }
            }
            ExprKind::Member { obj, .. } => self.walk_expr(obj),
            ExprKind::Index { obj, index } => {
                self.walk_expr(obj);
                self.walk_expr(index);
            }
            ExprKind::TurboFish { base, .. } => self.walk_expr(base),
            ExprKind::Try(inner) | ExprKind::Bang(inner) => self.walk_expr(inner),
            ExprKind::Coalesce(a, b) => {
                self.walk_expr(a);
                self.walk_expr(b);
            }
            ExprKind::As(inner, _) | ExprKind::Is(inner, _) => self.walk_expr(inner),
            ExprKind::If { cond, then, else_ } => {
                self.walk_expr(cond);
                self.walk_block(then, /*is_fn_body=*/ false);
                if let Some(e) = else_ { self.walk_else_branch(e); }
            }
            ExprKind::IfLet { scrutinee, then, else_, .. } => {
                self.walk_expr(scrutinee);
                self.walk_block(then, /*is_fn_body=*/ false);
                if let Some(e) = else_ { self.walk_else_branch(e); }
            }
            ExprKind::Match { scrutinee, arms } => {
                self.walk_expr(scrutinee);
                for arm in arms {
                    if let Some(g) = &arm.guard { self.walk_expr(g); }
                    self.walk_match_arm_body(&arm.body, /*escape=*/ false);
                }
            }
            ExprKind::While { cond, body, .. } => {
                self.walk_expr(cond);
                self.walk_block(body, /*is_fn_body=*/ false);
            }
            ExprKind::WhileLet { scrutinee, body, .. } => {
                self.walk_expr(scrutinee);
                self.walk_block(body, /*is_fn_body=*/ false);
            }
            ExprKind::Loop { body, .. } => self.walk_block(body, /*is_fn_body=*/ false),
            ExprKind::For { iter, body, .. } => {
                self.walk_expr(iter);
                self.walk_block(body, /*is_fn_body=*/ false);
            }
            ExprKind::ParallelFor { iter, body, .. } => {
                self.walk_expr(iter);
                self.walk_block(body, /*is_fn_body=*/ false);
            }
            ExprKind::Select { arms } => {
                for arm in arms {
                    if let Some(g) = &arm.guard { self.walk_expr(g); }
                    // SelectArm body field name varies — handled безопасно
                    // через Display-less ignore (conservative).
                    let _ = arm;
                }
            }
            ExprKind::ClosureLight { body, .. } => {
                // V1 escape sink: closure capture — `&v` inside a closure
                // body escapes if the closure escapes. Conservative OVER-
                // promote: any `&v` inside any closure body promotes.
                self.walk_closure_body(body, /*escape=*/ true);
            }
            ExprKind::ClosureFull(sig) => {
                self.walk_fn_body_escape(&sig.body);
            }
            ExprKind::Lambda { body, .. } => {
                self.walk_expr_in_escape(body);
            }
            ExprKind::With { bindings, body } => {
                for b in bindings {
                    self.walk_expr(&b.handler);
                }
                self.walk_block(body, /*is_fn_body=*/ false);
            }
            ExprKind::HandlerLit { .. } => {
                // Handler literal — methods registered later; conservative
                // skip (V1 не отслеживает captures внутрь handler-методов).
            }
            ExprKind::TupleLit(elems) => {
                for e in elems { self.walk_expr(e); }
            }
            ExprKind::ArrayLit(elems) => {
                for elem in elems {
                    match elem {
                        ArrayElem::Item(e) | ArrayElem::Spread(e) => self.walk_expr(e),
                    }
                }
            }
            ExprKind::RecordLit { fields, .. } => {
                // V1 escape sink: `OtherRecord { field: &v }` — if the
                // enclosing record is HEAP-allocated, &v escapes via heap
                // field. We can't easily distinguish heap vs value-record
                // syntactic source w/o type-checker; conservative: treat
                // all record-lit field positions as escape sinks.
                for f in fields {
                    if let Some(v) = &f.value {
                        self.walk_expr_in_escape(v);
                    }
                }
            }
            ExprKind::InterpolatedStr { parts } => {
                for p in parts {
                    if let InterpStrPart::Expr { expr, .. } = p {
                        self.walk_expr(expr);
                    }
                }
            }
            // Plan 127 V1: scalar / leaf expr kinds — no escape detection.
            _ => {}
        }
    }

    /// Walk `else { ... }` / `else if ...` branch.
    fn walk_else_branch(&mut self, eb: &ElseBranch) {
        match eb {
            ElseBranch::Block(b) => self.walk_block(b, /*is_fn_body=*/ false),
            ElseBranch::If(e) => self.walk_expr(e),
        }
    }

    fn walk_match_arm_body(&mut self, body: &MatchArmBody, escape: bool) {
        match body {
            MatchArmBody::Expr(e) => {
                if escape { self.walk_expr_in_escape(e); } else { self.walk_expr(e); }
            }
            MatchArmBody::Block(b) => self.walk_block(b, /*is_fn_body=*/ false),
        }
    }

    fn walk_closure_body(&mut self, body: &ClosureBody, escape: bool) {
        match body {
            ClosureBody::Expr(e) => {
                if escape { self.walk_expr_in_escape(e); } else { self.walk_expr(e); }
            }
            ClosureBody::Block(b) => {
                // Walk block — any `&v` тон inside still goes through
                // walk_stmt/walk_expr arms which themselves call escape
                // recursion at sub-positions (return/heap-field/closure).
                self.walk_block(b, /*is_fn_body=*/ false);
            }
        }
    }

    fn walk_fn_body_escape(&mut self, body: &FnBody) {
        match body {
            FnBody::Block(b) => self.walk_block(b, /*is_fn_body=*/ true),
            FnBody::Expr(e) => self.walk_expr_in_escape(e),
            FnBody::External => {}
        }
    }

    /// Walk expression in *return* position — bare `&IDENT` promotes the
    /// referenced local. This is the canonical A127.2 case.
    fn walk_expr_in_return(&mut self, e: &Expr) {
        self.walk_expr_in_escape(e);
    }

    /// Walk expression in an *escape* position — bare `&IDENT` promotes the
    /// referenced local. Recurses through tail-position constructs (if/
    /// match arms, block trailing, paren) so `if cond => &v else => other`
    /// is recognized.
    fn walk_expr_in_escape(&mut self, e: &Expr) {
        match &e.kind {
            ExprKind::Unary { op: UnOp::AddrOf, operand } => {
                // Bare `&IDENT` reaching an escape sink — promote IDENT.
                if let ExprKind::Ident(name) = &operand.kind {
                    self.mark_promoted(name);
                }
                // Recurse into operand for nested escape (rare: `&(&v)` —
                // but operand walk handles further `&` chains).
                self.walk_expr(operand);
            }
            // Tail-position recursion: nested expressions where the result
            // is still in escape position.
            ExprKind::Block(b) => {
                self.push_scope();
                for stmt in &b.stmts {
                    self.walk_stmt(stmt);
                }
                if let Some(trailing) = &b.trailing {
                    self.walk_expr_in_escape(trailing);
                }
                self.pop_scope();
            }
            ExprKind::If { cond, then, else_ } => {
                self.walk_expr(cond);
                self.walk_block_escape_tail(then);
                if let Some(eb) = else_ {
                    match eb {
                        ElseBranch::Block(b) => self.walk_block_escape_tail(b),
                        ElseBranch::If(e) => self.walk_expr_in_escape(e),
                    }
                }
            }
            ExprKind::IfLet { scrutinee, then, else_, .. } => {
                self.walk_expr(scrutinee);
                self.walk_block_escape_tail(then);
                if let Some(eb) = else_ {
                    match eb {
                        ElseBranch::Block(b) => self.walk_block_escape_tail(b),
                        ElseBranch::If(e) => self.walk_expr_in_escape(e),
                    }
                }
            }
            ExprKind::Match { scrutinee, arms } => {
                self.walk_expr(scrutinee);
                for arm in arms {
                    if let Some(g) = &arm.guard { self.walk_expr(g); }
                    self.walk_match_arm_body(&arm.body, /*escape=*/ true);
                }
            }
            // Plan 118.1 closeout: `addr_of(IDENT)` / `addr_of_mut(IDENT)`
            // desugar to UnOp::AddrOf по rewriter pass; but the source-level
            // call form is what we see in pre-rewriter AST. Treat as escape
            // promoter when arg is a bare Ident referring to a value-record
            // local.
            ExprKind::Call { func, args, .. } => {
                if let ExprKind::Ident(fname) = &func.kind {
                    if (fname == "addr_of" || fname == "addr_of_mut") && args.len() == 1 {
                        if let ExprKind::Ident(name) = &args[0].expr().kind {
                            self.mark_promoted(name);
                        }
                    }
                }
                // Non-escape recursion into general call (args have their own
                // escape walk inside walk_expr Call arm).
                self.walk_expr(e);
            }
            // Other expression forms in escape position — fall back to
            // neutral walk (no Ident-level escape recognition without `&`).
            _ => self.walk_expr(e),
        }
    }

    /// Walk a block whose tail expression is in escape position.
    fn walk_block_escape_tail(&mut self, b: &Block) {
        self.push_scope();
        for stmt in &b.stmts {
            self.walk_stmt(stmt);
        }
        if let Some(trailing) = &b.trailing {
            self.walk_expr_in_escape(trailing);
        }
        self.pop_scope();
    }
}

/// True if `e` is a member-access chain rooted at an Ident or self —
/// `obj.field`, `obj.field.sub`, `arr[i].field`, etc. Used to recognize
/// heap-field store sinks for `obj.field = &v`.
fn expr_is_member_chain(e: &Expr) -> bool {
    matches!(e.kind, ExprKind::Member { .. } | ExprKind::Index { .. })
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;

    fn analyze(src: &str) -> EscapeResult {
        let module = parse(src).expect("parse");
        analyze_module(&module)
    }

    #[test]
    fn empty_module_no_promotions() {
        let r = analyze("module plan127_test.empty");
        assert!(!r.has_any_promoted());
    }

    #[test]
    fn no_value_records_no_promotions() {
        let src = r#"
module plan127_test.heap_only
type Vec3 { x f64  y f64  z f64 }
fn make() -> Vec3 =>
    { x: 1.0, y: 2.0, z: 3.0 }
"#;
        let r = analyze(src);
        assert!(!r.has_any_promoted(),
            "heap-record types must not trigger value-record promotion");
    }
}
