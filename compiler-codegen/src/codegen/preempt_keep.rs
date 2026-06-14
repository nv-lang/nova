// Plan 143.2 — Leaf function-entry preempt-check elision  `[M-opt-leaf-preempt-entry-elision]`
//
// Source-level, whole-program pre-pass computing the set of functions that
// MUST keep their function-prologue `nova_preempt_check();` (Plan 44.7,
// emit_c.rs emit_fn). A fiber can run unboundedly WITHOUT yielding only
// through a loop (its back-edge check is preserved separately) or through
// recursion (a cycle in the call graph). Therefore the prologue entry-check
// is safe to ELIDE iff the function is provably NOT on any call-graph cycle
// AND does not reach an unknown callee (indirect/FFI) AND is not used as a
// first-class value (address-taken → target of indirect calls).
//
// SOUNDNESS — the analysis builds an OVER-APPROXIMATION of the real
// (post-monomorphization) call graph at the source level:
//   * every real direct edge is present (resolved by name + receiver +
//     param-signature; ambiguous overloads add edges to ALL candidates →
//     superset);
//   * any callee we cannot statically resolve to a known FnDecl, any
//     indirect/closure/fn-pointer call, any FFI/extern call, and any
//     address-taken function are conservatively flagged → KEEP.
// An over-approximated edge set yields an over-approximated cycle set, so
// every cycle in the REAL graph is present here too and gets at least one
// KEEP member. Spurious edges only ADD KEEP (lose optimization, stay sound).
// KEEP status computed on the source template is inherited by every
// monomorphized instance (recursion/indirect/FFI/address-taken are source
// properties), so no instance-level cycle is ever missed.
//
// CONSERVATIVE DEFAULT: at any doubt the function is placed in KEEP. The
// elision NEVER fires on an unproven case.

use crate::ast::*;
use std::collections::{HashMap, HashSet};

/// A stable, source-level identity for a callable node in the graph.
/// Free fn:   `fn::{name}::{param_sig}`
/// Method:    `{recv_type}::{name}::{param_sig}`
/// `param_sig` renders the source-written parameter TypeRefs; generics /
/// unresolved types render as `?` (so distinct concrete overloads stay
/// distinct, but two `?`-shaped overloads conservatively collapse).
pub type FnKey = String;

/// Result of the pre-pass: the set of FnKeys whose prologue preempt-check
/// MUST be kept (emitted). A function whose key is NOT in this set may have
/// its entry-check elided.
#[derive(Debug, Default, Clone)]
pub struct PreemptKeepSet {
    keep: HashSet<FnKey>,
    /// True once `compute_preempt_keep_set` ran over a non-empty universe.
    /// `false` for a default/empty set — callers MUST then KEEP everything.
    populated: bool,
}

impl PreemptKeepSet {
    /// Whether the given FnDecl must keep its prologue preempt-check.
    ///
    /// CONSERVATIVE CONTRACT: the analysis is only trusted to ELIDE when the
    /// pre-pass actually ran over the program (see [`populated`]). Callers
    /// therefore gate on `!populated() || must_keep(...)` — when the universe
    /// was never populated, EVERY function is kept. Within a populated set,
    /// every emitted FnDecl was registered as a node, so a queried key is
    /// always present and `contains` reflects the real KEEP decision. A key
    /// that is genuinely unknown to a populated set (a function emitted from
    /// outside the registered universe — none exist today) returns `false`
    /// here; such a path would be unsound, so any future emit site that lowers
    /// a not-registered function MUST keep unconditionally (see
    /// `emit_prologue_preempt_check_unconditional` in emit_c.rs) rather than
    /// consult this method.
    pub fn must_keep(&self, key: &FnKey) -> bool {
        self.keep.contains(key)
    }

    /// True when the pre-pass actually ran (non-empty universe). When the
    /// analysis was never populated (e.g. a CEmitter constructed directly in
    /// a unit test, or `compute` produced nothing), callers MUST treat every
    /// function as KEEP. Tracked via `populated`.
    pub fn populated(&self) -> bool {
        self.populated
    }
}

// Internal flags accumulated per node during the walk.
#[derive(Default, Clone)]
struct NodeFlags {
    makes_indirect: bool,
    makes_ffi: bool,
    address_taken: bool,
    edges: HashSet<FnKey>,
}

/// Render a parameter list to a stable signature string. Unknown / generic
/// types render as `?`. Used both for node keys and for overload matching.
fn param_sig(params: &[Param]) -> String {
    let mut parts: Vec<String> = Vec::with_capacity(params.len());
    for p in params {
        parts.push(typeref_sig(&p.ty));
    }
    parts.join(",")
}

/// Stable string form of a source TypeRef, sufficient to distinguish
/// concrete scalar overloads (`f32` vs `f64` vs `int`). Anything we cannot
/// render to a concrete primitive name renders to `?` (conservative — two
/// `?` overloads collapse, which only adds edges → KEEP).
fn typeref_sig(t: &TypeRef) -> String {
    match t {
        // Strip compile-time modifier wrappers — they do not change the
        // overload identity for our purposes (`ro f32` ≡ `f32` for dispatch).
        TypeRef::Readonly(inner, _) | TypeRef::Mut(inner, _) | TypeRef::Unsafe(inner, _) => {
            typeref_sig(inner)
        }
        TypeRef::Named { path, generics, .. } => {
            let name = path.join(".");
            if generics.is_empty() {
                name
            } else {
                // Generic application: render head + args. Concrete scalars
                // never have generics, so this never collapses the f32/f64
                // scalar overloads we care about.
                format!("{}<{}>", name, generics.iter().map(typeref_sig).collect::<Vec<_>>().join(","))
            }
        }
        TypeRef::Array(inner, _) => format!("[]{}", typeref_sig(inner)),
        TypeRef::FixedArray(n, inner, _) => format!("[{}]{}", n, typeref_sig(inner)),
        TypeRef::Tuple(elems, _) => {
            format!("({})", elems.iter().map(typeref_sig).collect::<Vec<_>>().join(","))
        }
        TypeRef::Unit(_) => "()".to_string(),
        // Pointers / funcs / protocols / anything else: not needed to
        // distinguish the scalar-forwarder overloads; collapse to `?`.
        _ => "?".to_string(),
    }
}

/// Best-effort, purely source-syntactic approximation of an expression's
/// TYPE signature, for overload disambiguation of resolved calls. Returns
/// `Some(sig)` only when source-evident (literal / `as T` cast / a call whose
/// resolved callee has a single, concrete, non-generic return type). `None`
/// means "unknown" — callers then edge to ALL candidate overloads
/// (conservative superset). NEVER wrong-narrows: an uncertain arg yields
/// `None` (KEEP-leaning), never a guessed concrete type.
fn approx_arg_sig_lit(e: &Expr) -> Option<String> {
    match &e.kind {
        ExprKind::As(_, ty) => Some(typeref_sig(ty)),
        ExprKind::IntLit(_) => Some("int".to_string()),
        ExprKind::FloatLit(_) => Some("f64".to_string()),
        ExprKind::BoolLit(_) => Some("bool".to_string()),
        ExprKind::StrLit(_) | ExprKind::InterpolatedStr { .. } => Some("str".to_string()),
        ExprKind::CharLit(_) => Some("char".to_string()),
        _ => None,
    }
}

/// Whether a candidate's param signature is COMPATIBLE with the approximated
/// argument signatures. `None` arg = unknown → matches anything. Differing
/// arity → no match. A known arg sig must equal the param sig (or the param
/// sig is `?` / generic, which matches anything).
fn overload_compatible(param_sig_str: &str, approx_args: &[Option<String>]) -> bool {
    let param_parts: Vec<&str> = if param_sig_str.is_empty() {
        Vec::new()
    } else {
        param_sig_str.split(',').collect()
    };
    if param_parts.len() != approx_args.len() {
        return false;
    }
    for (pp, aa) in param_parts.iter().zip(approx_args.iter()) {
        match aa {
            None => continue, // unknown arg → compatible (conservative)
            Some(a) => {
                if *pp == "?" || pp.contains('<') {
                    continue; // generic / unrendered param → compatible
                }
                if pp != a {
                    return false;
                }
            }
        }
    }
    true
}

/// Builder for the call-graph + flags.
struct Analyzer<'a> {
    /// All non-external FnDecls in the program, keyed by FnKey.
    nodes: HashMap<FnKey, &'a FnDecl>,
    /// Free-fn name → list of (key, FnDecl) overloads.
    free_by_name: HashMap<String, Vec<(FnKey, &'a FnDecl)>>,
    /// (recv_type, method_name) → list of (key, FnDecl) overloads.
    method_by_name: HashMap<(String, String), Vec<(FnKey, &'a FnDecl)>>,
    /// Names of all external (FFI) free functions.
    external_names: HashSet<String>,
    /// Names of all external methods, keyed by (recv_type, name).
    external_methods: HashSet<(String, String)>,
    /// Per-node accumulated flags + edges.
    flags: HashMap<FnKey, NodeFlags>,
}

impl<'a> Analyzer<'a> {
    fn new() -> Self {
        Analyzer {
            nodes: HashMap::new(),
            free_by_name: HashMap::new(),
            method_by_name: HashMap::new(),
            external_names: HashSet::new(),
            external_methods: HashSet::new(),
            flags: HashMap::new(),
        }
    }

    /// Register a FnDecl as a node (or as an external name).
    fn register(&mut self, f: &'a FnDecl) {
        // External / no-body → FFI target, not a graph node we emit a check for.
        let is_external = f.is_external || matches!(f.body, FnBody::External);
        if let Some(recv) = &f.receiver {
            let recv_ty = recv.type_name.clone();
            if is_external {
                self.external_methods.insert((recv_ty, f.name.clone()));
                return;
            }
            let key = method_key(&recv_ty, &f.name, &f.params);
            self.nodes.insert(key.clone(), f);
            self.method_by_name
                .entry((recv_ty, f.name.clone()))
                .or_default()
                .push((key.clone(), f));
            self.flags.entry(key).or_default();
        } else {
            if is_external {
                self.external_names.insert(f.name.clone());
                return;
            }
            let key = free_key(&f.name, &f.params);
            self.nodes.insert(key.clone(), f);
            self.free_by_name
                .entry(f.name.clone())
                .or_default()
                .push((key.clone(), f));
            self.flags.entry(key).or_default();
        }
    }

    fn flag_mut(&mut self, key: &FnKey) -> &mut NodeFlags {
        self.flags.entry(key.clone()).or_default()
    }

    /// Walk a function body, attributing edges/flags to `cur_key`.
    /// `recv_type` = enclosing method receiver type (for `@`-self calls).
    fn analyze_fn(&mut self, key: &FnKey, f: &'a FnDecl) {
        let recv_type = f.receiver.as_ref().map(|r| r.type_name.clone());
        match &f.body {
            FnBody::Block(b) => self.walk_block(key, recv_type.as_deref(), b),
            FnBody::Expr(e) => self.walk_expr(key, recv_type.as_deref(), e),
            FnBody::External => {}
        }
    }

    fn walk_block(&mut self, cur: &FnKey, recv: Option<&str>, b: &Block) {
        for s in &b.stmts {
            self.walk_stmt(cur, recv, s);
        }
        if let Some(t) = &b.trailing {
            self.walk_expr(cur, recv, t);
        }
    }

    fn walk_stmt(&mut self, cur: &FnKey, recv: Option<&str>, s: &Stmt) {
        match s {
            Stmt::Let(d) => self.walk_expr(cur, recv, &d.value),
            Stmt::Const(c) => self.walk_expr(cur, recv, &c.value),
            Stmt::Expr(e) => self.walk_expr(cur, recv, e),
            Stmt::Assign { target, value, .. } => {
                self.walk_expr(cur, recv, target);
                self.walk_expr(cur, recv, value);
            }
            Stmt::TupleAssign { lhs, rhs, .. } => {
                for e in lhs { self.walk_expr(cur, recv, e); }
                for e in rhs { self.walk_expr(cur, recv, e); }
            }
            Stmt::Return { value: Some(v), .. } => self.walk_expr(cur, recv, v),
            Stmt::Return { value: None, .. } => {}
            Stmt::Throw { value, .. } => self.walk_expr(cur, recv, value),
            Stmt::Defer { body, .. }
            | Stmt::ErrDefer { body, .. }
            | Stmt::OkDefer { body, .. }
            | Stmt::DeferWithResult { body, .. } => self.walk_expr(cur, recv, body),
            Stmt::ConsumeScope { init, body, .. } => {
                self.walk_expr(cur, recv, init);
                self.walk_block(cur, recv, body);
            }
            Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => {
                self.walk_expr(cur, recv, expr);
            }
            Stmt::Apply { args, .. } => {
                for a in args { self.walk_expr(cur, recv, a); }
            }
            Stmt::Calc { steps, .. } => {
                for st in steps { self.walk_expr(cur, recv, &st.expr); }
            }
            Stmt::Break(_) | Stmt::Continue(_) | Stmt::Reveal { .. } => {}
        }
    }

    fn walk_expr(&mut self, cur: &FnKey, recv: Option<&str>, e: &Expr) {
        match &e.kind {
            ExprKind::Call { func, args, trailing } => {
                self.handle_call(cur, recv, func, args);
                // Walk arguments (they may contain nested calls AND
                // address-taken fn references).
                for a in args {
                    self.walk_expr(cur, recv, a.expr());
                }
                if let Some(t) = trailing {
                    match t {
                        Trailing::Block(b) => {
                            // Trailing block runs synchronously in the caller
                            // (e.g. with_timeout / retry DSL). Conservatively
                            // mark indirect: its callee semantics depend on the
                            // DSL fn, which may re-invoke the block.
                            self.flag_mut(cur).makes_indirect = true;
                            self.walk_block(cur, recv, b);
                        }
                        Trailing::Fn(fsb) => {
                            self.flag_mut(cur).makes_indirect = true;
                            self.walk_fnbody(cur, recv, &fsb.body);
                        }
                        Trailing::LegacyBlockWithParams(tb) => {
                            self.flag_mut(cur).makes_indirect = true;
                            self.walk_block(cur, recv, &tb.body);
                        }
                    }
                }
            }
            ExprKind::Ident(name) => {
                // Bare fn name in a NON-call (value) position → address-taken.
                self.mark_address_taken_free(name);
            }
            ExprKind::Path(parts) => {
                // `Type.method` / `module.func` used as a value (not the
                // func of a Call) → address-taken target. Mark conservatively.
                self.mark_address_taken_path(parts);
            }
            ExprKind::Member { obj, .. } => self.walk_expr(cur, recv, obj),
            ExprKind::Index { obj, index } => {
                self.walk_expr(cur, recv, obj);
                self.walk_expr(cur, recv, index);
            }
            ExprKind::TurboFish { base, .. } => self.walk_expr(cur, recv, base),
            ExprKind::ArrayLit(elems) => {
                for el in elems {
                    match el {
                        ArrayElem::Item(x) | ArrayElem::Spread(x) => self.walk_expr(cur, recv, x),
                    }
                }
            }
            ExprKind::MapLit { elems, .. } => {
                for el in elems {
                    match el {
                        MapElem::Pair(k, v) => {
                            self.walk_expr(cur, recv, k);
                            self.walk_expr(cur, recv, v);
                        }
                        MapElem::Spread(x) => self.walk_expr(cur, recv, x),
                    }
                }
            }
            ExprKind::RecordLit { fields, .. } => {
                for fld in fields {
                    if let Some(v) = &fld.value {
                        self.walk_expr(cur, recv, v);
                    } else {
                        // shorthand `{ name }` — name is a value, could be a
                        // fn reference → address-taken.
                        self.mark_address_taken_free(&fld.name);
                    }
                }
            }
            ExprKind::TupleLit(elems) => {
                for x in elems { self.walk_expr(cur, recv, x); }
            }
            ExprKind::Binary { left, right, .. } => {
                self.walk_expr(cur, recv, left);
                self.walk_expr(cur, recv, right);
            }
            ExprKind::Unary { operand, .. } => self.walk_expr(cur, recv, operand),
            ExprKind::As(inner, _) | ExprKind::Is(inner, _)
            | ExprKind::Try(inner) | ExprKind::Bang(inner) => self.walk_expr(cur, recv, inner),
            ExprKind::Coalesce(l, r) => {
                self.walk_expr(cur, recv, l);
                self.walk_expr(cur, recv, r);
            }
            ExprKind::If { cond, then, else_ } => {
                self.walk_expr(cur, recv, cond);
                self.walk_block(cur, recv, then);
                self.walk_else(cur, recv, else_);
            }
            ExprKind::IfLet { scrutinee, then, else_, .. } => {
                self.walk_expr(cur, recv, scrutinee);
                self.walk_block(cur, recv, then);
                self.walk_else(cur, recv, else_);
            }
            ExprKind::Match { scrutinee, arms } => {
                self.walk_expr(cur, recv, scrutinee);
                for arm in arms {
                    if let Some(g) = &arm.guard {
                        self.walk_expr(cur, recv, g);
                    }
                    match &arm.body {
                        MatchArmBody::Expr(x) => self.walk_expr(cur, recv, x),
                        MatchArmBody::Block(b) => self.walk_block(cur, recv, b),
                    }
                }
            }
            ExprKind::For { iter, body, invariants, decreases, .. } => {
                self.walk_expr(cur, recv, iter);
                self.walk_block(cur, recv, body);
                for inv in invariants { self.walk_expr(cur, recv, inv); }
                if let Some(d) = decreases { self.walk_expr(cur, recv, d); }
            }
            ExprKind::ParallelFor { iter, body, .. } => {
                // parallel-for spawns fibers running `body` indirectly (via the
                // scheduler) — treat as indirect (unknown call target).
                self.flag_mut(cur).makes_indirect = true;
                self.walk_expr(cur, recv, iter);
                self.walk_block(cur, recv, body);
            }
            ExprKind::While { cond, body, invariants, decreases }
            | ExprKind::WhileLet { scrutinee: cond, body, invariants, decreases, .. } => {
                self.walk_expr(cur, recv, cond);
                self.walk_block(cur, recv, body);
                for inv in invariants { self.walk_expr(cur, recv, inv); }
                if let Some(d) = decreases { self.walk_expr(cur, recv, d); }
            }
            ExprKind::Loop { body, invariants, decreases } => {
                self.walk_block(cur, recv, body);
                for inv in invariants { self.walk_expr(cur, recv, inv); }
                if let Some(d) = decreases { self.walk_expr(cur, recv, d); }
            }
            ExprKind::Select { arms } => {
                // select over channels parks/wakes — conservatively a safepoint
                // but the recv/send targets are runtime — treat bodies normally;
                // mark indirect (channel ops are FFI-ish runtime).
                self.flag_mut(cur).makes_indirect = true;
                for arm in arms {
                    if let Some(g) = &arm.guard { self.walk_expr(cur, recv, g); }
                    match &arm.op {
                        SelectOp::Recv { chan, .. } => self.walk_expr(cur, recv, chan),
                        SelectOp::Send { chan, value } => {
                            self.walk_expr(cur, recv, chan);
                            self.walk_expr(cur, recv, value);
                        }
                        SelectOp::Default => {}
                    }
                    self.walk_block(cur, recv, &arm.body);
                }
            }
            ExprKind::Lambda { body, .. } => {
                // Defining a closure does not itself call anything, but the
                // closure body is reachable indirectly when later invoked.
                // The act of CREATING a closure here does not force KEEP; the
                // INVOCATION (an indirect call elsewhere) does. Still walk the
                // body so address-taken refs inside are caught.
                self.walk_expr(cur, recv, body);
            }
            ExprKind::ClosureLight { body, .. } => match body {
                ClosureBody::Expr(x) => self.walk_expr(cur, recv, x),
                ClosureBody::Block(b) => self.walk_block(cur, recv, b),
            },
            ExprKind::ClosureFull(fsb) => self.walk_fnbody(cur, recv, &fsb.body),
            ExprKind::With { bindings, body } => {
                for wb in bindings { self.walk_expr(cur, recv, &wb.handler); }
                // effect handlers are invoked indirectly → KEEP the caller.
                self.flag_mut(cur).makes_indirect = true;
                self.walk_block(cur, recv, body);
            }
            ExprKind::HandlerLit { methods, .. } | ExprKind::ProtocolLit { methods, .. } => {
                for m in methods {
                    match &m.body {
                        HandlerMethodBody::Expr(x) => self.walk_expr(cur, recv, x),
                        HandlerMethodBody::Block(b) => self.walk_block(cur, recv, b),
                    }
                }
            }
            ExprKind::Interrupt(opt) => {
                if let Some(x) = opt { self.walk_expr(cur, recv, x); }
            }
            ExprKind::Forbid { body, .. } | ExprKind::Realtime { body, .. } => {
                self.walk_block(cur, recv, body);
            }
            ExprKind::Range { start, end, .. } => {
                if let Some(s) = start { self.walk_expr(cur, recv, s); }
                if let Some(en) = end { self.walk_expr(cur, recv, en); }
            }
            ExprKind::Forall { range, body, .. } | ExprKind::Exists { range, body, .. } => {
                self.walk_expr(cur, recv, range);
                self.walk_expr(cur, recv, body);
            }
            ExprKind::Block(b) => self.walk_block(cur, recv, b),
            ExprKind::Spawn(inner) => {
                // spawn runs `inner` on a fiber via the scheduler (indirect).
                self.flag_mut(cur).makes_indirect = true;
                self.walk_expr(cur, recv, inner);
            }
            ExprKind::Supervised { body, cancel } => {
                self.flag_mut(cur).makes_indirect = true;
                if let Some(c) = cancel { self.walk_expr(cur, recv, c); }
                self.walk_block(cur, recv, body);
            }
            ExprKind::Detach(b) | ExprKind::Blocking(b) => {
                // detach → indirect fiber; blocking → FFI/threadpool offload.
                self.flag_mut(cur).makes_indirect = true;
                self.flag_mut(cur).makes_ffi = true;
                self.walk_block(cur, recv, b);
            }
            ExprKind::Throw(inner) => self.walk_expr(cur, recv, inner),
            ExprKind::TaggedTemplate { tag, args, .. } => {
                self.walk_expr(cur, recv, tag);
                for a in args { self.walk_expr(cur, recv, a); }
            }
            // Leaves with no sub-expressions / no callee.
            ExprKind::IntLit(_) | ExprKind::FloatLit(_) | ExprKind::StrLit(_)
            | ExprKind::InterpolatedStr { .. } | ExprKind::BoolLit(_)
            | ExprKind::UnitLit | ExprKind::CharLit(_) | ExprKind::NullPtrLit
            | ExprKind::SelfAccess => {}
        }
    }

    fn walk_fnbody(&mut self, cur: &FnKey, recv: Option<&str>, b: &FnBody) {
        match b {
            FnBody::Block(blk) => self.walk_block(cur, recv, blk),
            FnBody::Expr(e) => self.walk_expr(cur, recv, e),
            FnBody::External => {}
        }
    }

    fn walk_else(&mut self, cur: &FnKey, recv: Option<&str>, else_: &Option<ElseBranch>) {
        match else_ {
            Some(ElseBranch::Block(b)) => self.walk_block(cur, recv, b),
            Some(ElseBranch::If(e)) => self.walk_expr(cur, recv, e),
            None => {}
        }
    }

    /// Approximate an argument expression's type signature for overload
    /// disambiguation. Extends the literal/cast approximation with resolution
    /// of a callee's concrete return type (e.g. `str.from(x)` → `str`), which
    /// is what breaks the `append(f64)` self-call into the `append(str)`
    /// overload (instead of a spurious self-loop over all `append`s).
    /// Conservative: any uncertainty yields `None`.
    fn approx_arg_sig(&self, e: &Expr) -> Option<String> {
        if let Some(s) = approx_arg_sig_lit(e) {
            return Some(s);
        }
        match &e.kind {
            // A call whose callee resolves UNAMBIGUOUSLY to a known FnDecl with
            // a single concrete return type → that return type.
            ExprKind::Call { func, .. } => self.callee_return_sig(func),
            ExprKind::TurboFish { base, .. } => self.approx_arg_sig(base),
            _ => None,
        }
    }

    /// Resolve the concrete return-type signature of a call's `func`, but only
    /// when the callee resolves to a SINGLE known FnDecl whose return type is a
    /// concrete (non-generic) named/array/tuple type. Otherwise `None`.
    ///
    /// Overloads are tolerated: when a name has MULTIPLE overloads they must
    /// AGREE on a single concrete return signature (e.g. every `str.from(_)`
    /// returns `str`) — otherwise `None`. `Self` returns resolve to the
    /// receiver type. This is what lets `append(f64)`'s `str.from(x)` argument
    /// resolve to `str` and break the spurious `append` self-loop.
    fn callee_return_sig(&self, func: &Expr) -> Option<String> {
        // Collect candidate FnDecls + their receiver type (for `Self`).
        let cands: Vec<(&FnDecl, Option<String>)> = match &func.kind {
            ExprKind::Path(parts) if parts.len() >= 2 => {
                let last = &parts[parts.len() - 1];
                let prev = &parts[parts.len() - 2];
                if let Some(v) = self.method_by_name.get(&(prev.clone(), last.clone())) {
                    v.iter().map(|(_, f)| (*f, Some(prev.clone()))).collect()
                } else if let Some(v) = self.free_by_name.get(last) {
                    v.iter().map(|(_, f)| (*f, None)).collect()
                } else {
                    return None;
                }
            }
            ExprKind::Ident(name) => {
                let v = self.free_by_name.get(name)?;
                v.iter().map(|(_, f)| (*f, None)).collect()
            }
            _ => return None,
        };
        if cands.is_empty() {
            return None;
        }
        let mut agreed: Option<String> = None;
        for (f, recv_ty) in &cands {
            let rt = f.return_type.as_ref()?;
            // Resolve `Self`/`@` return to the receiver type.
            let sig = match rt {
                TypeRef::Named { path, generics, .. }
                    if generics.is_empty() && path.len() == 1 && path[0] == "Self" =>
                {
                    match recv_ty {
                        Some(t) => t.clone(),
                        None => return None,
                    }
                }
                _ => typeref_sig(rt),
            };
            // Only trust concrete, fully-rendered signatures.
            if sig == "?" || sig.contains('?') || sig.contains('<') {
                return None;
            }
            match &agreed {
                None => agreed = Some(sig),
                Some(a) if *a == sig => {}
                Some(_) => return None, // overloads disagree → unknown
            }
        }
        agreed
    }

    /// Resolve a call's `func` to graph edges / flags on `cur`.
    fn handle_call(&mut self, cur: &FnKey, recv: Option<&str>, func: &Expr, args: &[CallArg]) {
        // Approximate argument signatures for overload disambiguation.
        let approx: Vec<Option<String>> = args.iter().map(|a| self.approx_arg_sig(a.expr())).collect();
        // Only positional/known args participate; named/spread args make the
        // arity uncertain → fall back to "edge to all overloads".
        let arity_certain = args.iter().all(|a| matches!(a, CallArg::Item(_)));

        match &func.kind {
            ExprKind::Member { obj, name } => {
                if matches!(obj.kind, ExprKind::SelfAccess) {
                    // `@name(args)` — self method call; receiver type known.
                    if let Some(rt) = recv {
                        self.resolve_method_edge(cur, rt, name, &approx, arity_certain);
                    } else {
                        // self call but receiver type somehow unknown → KEEP.
                        self.flag_mut(cur).makes_indirect = true;
                    }
                } else {
                    // Method call on a non-self receiver — receiver type is not
                    // source-evident in a pre-pass. Could resolve to any
                    // `*::name` overload (recursion possible) → conservatively
                    // KEEP the caller.
                    self.flag_mut(cur).makes_indirect = true;
                    // Walk obj for nested calls/address-taken.
                    // (obj walked by caller's walk_expr on args/children; here
                    // we walk it explicitly since func isn't otherwise walked.)
                    self.walk_expr(cur, recv, obj);
                }
            }
            ExprKind::Ident(name) => {
                // Either a free-fn call, an external (FFI) call, or a call
                // through a closure/fn-pointer variable bearing this name.
                if self.external_names.contains(name) {
                    self.flag_mut(cur).makes_ffi = true;
                } else if self.free_by_name.contains_key(name) {
                    self.resolve_free_edge(cur, name, &approx, arity_certain);
                } else {
                    // Unknown bare-ident callee: closure/fn-ptr local OR a
                    // cross-module fn whose decl we never saw. Cannot prove
                    // acyclicity → indirect (KEEP).
                    self.flag_mut(cur).makes_indirect = true;
                }
            }
            ExprKind::Path(parts) => {
                self.handle_path_call(cur, parts, &approx, arity_certain);
            }
            ExprKind::SelfAccess => {
                // `@(...)` — calling the receiver itself as a value (rare);
                // indirect.
                self.flag_mut(cur).makes_indirect = true;
            }
            ExprKind::TurboFish { base, .. } => {
                // `f[T](args)` / `Type[T].m(args)` — resolve through base.
                self.handle_call(cur, recv, base, args);
            }
            _ => {
                // Any other callee shape (call result, index, paren-expr, …)
                // is an indirect call — target not a static fn symbol → KEEP.
                self.flag_mut(cur).makes_indirect = true;
                self.walk_expr(cur, recv, func);
            }
        }
    }

    fn handle_path_call(&mut self, cur: &FnKey, parts: &[String], approx: &[Option<String>], arity_certain: bool) {
        if parts.len() >= 2 {
            // `Type.method` (static) or `module.func`. `Type` may be an
            // UPPER-case user type OR a lower-case primitive (`str`, `int`,
            // `char`) — so probe the method maps FIRST (case-agnostic), then
            // fall back to treating `last` as a free fn (module.func form).
            let last = &parts[parts.len() - 1];
            let prev = &parts[parts.len() - 2];
            // External static method on a known type?
            if self.external_methods.contains(&(prev.clone(), last.clone())) {
                self.flag_mut(cur).makes_ffi = true;
                return;
            }
            if self.method_by_name.contains_key(&(prev.clone(), last.clone())) {
                self.resolve_method_edge(cur, prev, last, approx, arity_certain);
                return;
            }
            // `module.func` → treat `last` as a free fn name.
            if self.external_names.contains(last) {
                self.flag_mut(cur).makes_ffi = true;
            } else if self.free_by_name.contains_key(last) {
                self.resolve_free_edge(cur, last, approx, arity_certain);
            } else {
                // Unknown Type.method / module.func → cross-module / unresolved → KEEP.
                self.flag_mut(cur).makes_indirect = true;
            }
            return;
        }
        // Single-segment Path used as a call — unusual; treat as free fn name.
        if let Some(name) = parts.first() {
            if self.external_names.contains(name) {
                self.flag_mut(cur).makes_ffi = true;
            } else if self.free_by_name.contains_key(name) {
                self.resolve_free_edge(cur, name, approx, arity_certain);
            } else {
                self.flag_mut(cur).makes_indirect = true;
            }
        }
    }

    /// Add edges from `cur` to the matching free-fn overloads of `name`.
    fn resolve_free_edge(&mut self, cur: &FnKey, name: &str, approx: &[Option<String>], arity_certain: bool) {
        let candidates: Vec<FnKey> = match self.free_by_name.get(name) {
            Some(v) => v.iter().map(|(k, _)| k.clone()).collect(),
            None => return,
        };
        self.add_candidate_edges(cur, name, &candidates, approx, arity_certain);
    }

    /// Add edges from `cur` to the matching method overloads of `recv::name`.
    fn resolve_method_edge(&mut self, cur: &FnKey, recv_ty: &str, name: &str, approx: &[Option<String>], arity_certain: bool) {
        let candidates: Vec<FnKey> = match self.method_by_name.get(&(recv_ty.to_string(), name.to_string())) {
            Some(v) => v.iter().map(|(k, _)| k.clone()).collect(),
            None => {
                // No known overload on this receiver type → unresolved → KEEP.
                self.flag_mut(cur).makes_indirect = true;
                return;
            }
        };
        self.add_candidate_edges(cur, name, &candidates, approx, arity_certain);
    }

    /// Shared overload-edge logic: edge to the subset of `candidates` whose
    /// param-signature is compatible with the approximated args. If arity is
    /// uncertain (named/spread args) OR no candidate matched, edge to ALL
    /// candidates (conservative superset).
    fn add_candidate_edges(&mut self, cur: &FnKey, _name: &str, candidates: &[FnKey], approx: &[Option<String>], arity_certain: bool) {
        if candidates.is_empty() {
            self.flag_mut(cur).makes_indirect = true;
            return;
        }
        let mut chosen: Vec<FnKey> = Vec::new();
        if arity_certain {
            for k in candidates {
                // Extract the param_sig portion of the key (after last "::").
                let psig = key_param_sig(k);
                if overload_compatible(psig, approx) {
                    chosen.push(k.clone());
                }
            }
        }
        if chosen.is_empty() {
            // Ambiguous / uncertain → superset: edge to every candidate.
            chosen = candidates.to_vec();
        }
        let f = self.flag_mut(cur);
        for k in chosen {
            f.edges.insert(k);
        }
    }

    fn mark_address_taken_free(&mut self, name: &str) {
        if let Some(v) = self.free_by_name.get(name) {
            let keys: Vec<FnKey> = v.iter().map(|(k, _)| k.clone()).collect();
            for k in keys {
                self.flag_mut(&k).address_taken = true;
            }
        }
    }

    fn mark_address_taken_path(&mut self, parts: &[String]) {
        if parts.len() >= 2 {
            let last = &parts[parts.len() - 1];
            let prev = &parts[parts.len() - 2];
            // `Type.method` value-ref (case-agnostic — primitives are lower).
            if let Some(v) = self.method_by_name.get(&(prev.clone(), last.clone())) {
                let keys: Vec<FnKey> = v.iter().map(|(k, _)| k.clone()).collect();
                for k in keys {
                    self.flag_mut(&k).address_taken = true;
                }
            } else {
                // `module.func` value-ref.
                self.mark_address_taken_free(last);
            }
        } else if let Some(name) = parts.first() {
            self.mark_address_taken_free(name);
        }
    }
}

fn free_key(name: &str, params: &[Param]) -> FnKey {
    format!("fn::{}::{}", name, param_sig(params))
}

fn method_key(recv_ty: &str, name: &str, params: &[Param]) -> FnKey {
    format!("{}::{}::{}", recv_ty, name, param_sig(params))
}

/// Extract the param-signature portion of a FnKey (after the last `::`).
fn key_param_sig(k: &FnKey) -> &str {
    match k.rfind("::") {
        Some(i) => &k[i + 2..],
        None => "",
    }
}

/// Tarjan SCC over the edge map. Returns the set of node keys that are on a
/// cycle (a self-loop, or an SCC of size > 1).
fn cycle_members(flags: &HashMap<FnKey, NodeFlags>) -> HashSet<FnKey> {
    // Index nodes.
    let nodes: Vec<&FnKey> = flags.keys().collect();
    let mut idx_of: HashMap<&FnKey, usize> = HashMap::new();
    for (i, n) in nodes.iter().enumerate() {
        idx_of.insert(*n, i);
    }
    let n = nodes.len();

    #[derive(Clone)]
    struct TState {
        index: i64,
        lowlink: i64,
        on_stack: bool,
    }
    let mut state: Vec<TState> = vec![TState { index: -1, lowlink: -1, on_stack: false }; n];
    let mut stack: Vec<usize> = Vec::new();
    let mut counter: i64 = 0;
    let mut result: HashSet<FnKey> = HashSet::new();
    // Self-loops: a node with an edge to itself is a cycle regardless of SCC.
    let mut self_loop: Vec<bool> = vec![false; n];

    // Precompute adjacency by index (only edges that point to known nodes).
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (u, name) in nodes.iter().enumerate() {
        if let Some(nf) = flags.get(*name) {
            for e in &nf.edges {
                if let Some(&v) = idx_of.get(e) {
                    adj[u].push(v);
                    if v == u {
                        self_loop[u] = true;
                    }
                }
            }
        }
    }

    // Iterative Tarjan (recursion could blow the stack on deep graphs).
    // Frame: (node, next-adjacency-cursor).
    for start in 0..n {
        if state[start].index != -1 {
            continue;
        }
        let mut call_stack: Vec<(usize, usize)> = vec![(start, 0)];
        while let Some(&(u, ci)) = call_stack.last() {
            if ci == 0 {
                state[u].index = counter;
                state[u].lowlink = counter;
                counter += 1;
                stack.push(u);
                state[u].on_stack = true;
            }
            if ci < adj[u].len() {
                let v = adj[u][ci];
                // advance cursor for u
                call_stack.last_mut().unwrap().1 += 1;
                if state[v].index == -1 {
                    call_stack.push((v, 0));
                } else if state[v].on_stack {
                    if state[v].index < state[u].lowlink {
                        state[u].lowlink = state[v].index;
                    }
                }
            } else {
                // Done with u: if root of SCC, pop component.
                if state[u].lowlink == state[u].index {
                    let mut comp: Vec<usize> = Vec::new();
                    loop {
                        let w = stack.pop().unwrap();
                        state[w].on_stack = false;
                        comp.push(w);
                        if w == u {
                            break;
                        }
                    }
                    if comp.len() > 1 {
                        for &w in &comp {
                            result.insert(nodes[w].clone());
                        }
                    }
                }
                call_stack.pop();
                // propagate lowlink to parent
                if let Some(&(p, _)) = call_stack.last() {
                    if state[u].lowlink < state[p].lowlink {
                        state[p].lowlink = state[u].lowlink;
                    }
                }
            }
        }
    }
    // Add self-loop nodes.
    for (i, sl) in self_loop.iter().enumerate() {
        if *sl {
            result.insert(nodes[i].clone());
        }
    }
    result
}

/// Compute the KEEP set over the whole program.
///
/// `all_fns` = every FnDecl reachable at codegen time (entry module items +
/// every peer file's `items_here`, covering imported modules). Duplicate
/// FnDecls (same item appearing via both `module.items` and a peer file) are
/// harmless — registration is idempotent on the key.
pub fn compute_preempt_keep_set<'a, I>(all_fns: I) -> PreemptKeepSet
where
    I: IntoIterator<Item = &'a FnDecl>,
{
    let mut an = Analyzer::new();
    // Collect once so we can iterate twice (register, then analyze).
    let fns: Vec<&'a FnDecl> = all_fns.into_iter().collect();
    for f in &fns {
        an.register(f);
    }
    // Analyze bodies (edges + flags). Iterate over the registered node set so
    // each key is analyzed once with its FnDecl.
    // Build (key, fn) work list from the node map to avoid borrow conflicts.
    let work: Vec<(FnKey, &'a FnDecl)> =
        an.nodes.iter().map(|(k, f)| (k.clone(), *f)).collect();
    for (key, f) in &work {
        an.analyze_fn(key, f);
    }

    // SCC → cycle members.
    let cycles = cycle_members(&an.flags);

    let mut keep: HashSet<FnKey> = HashSet::new();
    for (key, nf) in &an.flags {
        if nf.makes_indirect || nf.makes_ffi || nf.address_taken || cycles.contains(key) {
            keep.insert(key.clone());
        }
    }

    PreemptKeepSet { keep, populated: !an.nodes.is_empty() }
}

/// Public helper to compute a FnDecl's node key — used by emit_fn at the
/// prologue to consult the KEEP set with the SAME keying scheme.
pub fn fn_key(f: &FnDecl) -> FnKey {
    if let Some(recv) = &f.receiver {
        method_key(&recv.type_name, &f.name, &f.params)
    } else {
        free_key(&f.name, &f.params)
    }
}
