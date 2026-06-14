// Plan 144.0 — may-GC effect analysis  (closes Plan 144 §7.6 hole H4 / Q15)
//
// Source-level, whole-program COMPILE-TIME pre-pass computing, for every
// non-external function (at the source-template level — see SOUNDNESS below),
// whether it is provably `NoGC` (cannot trigger a GC safe-point: it neither
// allocates itself nor reaches any callee that might) or — conservatively —
// `MayGC`.  The lattice is two-point:
//
//          MayGC   (top, ⊤)   ← DEFAULT (soundness — H4)
//            │
//          NoGC    (bottom, ⊥) ← must be PROVEN
//
// This pass EMITS NOTHING into the generated C.  It is consulted only by the
// `gc-effect-analyze` introspection CLI and the unit tests below.  Plan 144
// Ф.2 wires consumption (frame-elision / write-back-skip) later, out of scope
// here — `emit_c.rs` MUST NOT call into this module during emission.
//
// STRUCTURE — this module is modelled CLOSELY on `preempt_keep.rs` (Plan
// 143.2): the same node/edge call-graph build, the same `FnKey`
// (`fn_key`/`free_key`/`method_key`) scheme, the same `param_sig` rendering,
// the same overload resolution (`approx_arg_sig` + `add_candidate_edges`), and
// the same iterative Tarjan.  Two deliberate DIVERGENCES from the template:
//
//   1. We need FULL SCC components (not just cycle membership) to propagate
//      MayGC *up the callers* via a reverse-topological pass over the SCC
//      condensation (Plan §3.4).  `preempt_keep::cycle_members` only reports
//      cycle membership; here `scc_components` returns the complete partition.
//
//   2. `address_taken` does NOT by itself seed MayGC (Plan §3.2 NB): being a
//      first-class value is not allocating.  Only the CALLER performing the
//      indirect call is MayGC, which is already captured by `makes_indirect`.
//      So this module drops the `address_taken` seed entirely.
//
// SOUNDNESS — the analysis builds an OVER-APPROXIMATION of the real
// (post-monomorphization) call graph at the source level:
//   * every real direct edge is present (resolved by name + receiver +
//     param-signature; ambiguous overloads add edges to ALL candidates →
//     superset);
//   * any callee we cannot statically resolve to a known FnDecl, any
//     indirect/closure/fn-pointer/`with`/`spawn`/`select`/`parallel-for`
//     call, and any FFI/extern call conservatively SEED MayGC;
//   * any expression that constructs / clones / boxes heap memory SEEDS
//     MayGC, and ANY AST node not on the provably-non-allocating allowlist is
//     treated as allocating (Plan §3.3 design principle: unknown → allocates).
// An over-approximated edge/seed set yields an over-approximated MayGC set, so
// every real may-GC instance is flagged.  Spurious edges/seeds only LOSE
// elision (NoGC → MayGC), never break soundness.  The MayGC verdict computed
// on the source template is inherited by every monomorphized / erased instance
// (allocation / indirect / FFI / unresolved are SOURCE properties — Plan
// §3.5), so no instance is ever wrongly proven NoGC.
//
// CONSERVATIVE DEFAULT: at any doubt the function is MayGC.  `is_no_gc` NEVER
// returns true on an unproven case.  When the universe was never populated,
// EVERYONE is MayGC (mirrors `PreemptKeepSet::populated`).

use crate::ast::*;
use std::collections::{HashMap, HashSet};

/// A stable, source-level identity for a callable node in the graph.
/// Free fn:   `fn::{name}::{param_sig}`
/// Method:    `{recv_type}::{name}::{param_sig}`
/// Identical scheme to `preempt_keep::FnKey` so the two analyses are
/// key-compatible.
pub type FnKey = String;

/// Result of the pre-pass: the set of FnKeys that are `MayGC`.  A function
/// whose key is NOT in this set (within a populated universe) is provably
/// `NoGC`.
#[derive(Debug, Default, Clone)]
pub struct MayGcSet {
    may_gc: HashSet<FnKey>,
    /// True once `compute_may_gc_set` ran over a non-empty universe.  `false`
    /// for a default/empty set — callers MUST then treat EVERY function as
    /// MayGC (no one is NoGC).
    populated: bool,
}

impl MayGcSet {
    /// Whether the given function is PROVABLY `NoGC`.
    ///
    /// CONSERVATIVE CONTRACT: only trusted to report `NoGC` when the pre-pass
    /// actually ran over the program (see [`populated`]).  When the universe
    /// was never populated, this returns `false` for everyone (all MayGC).
    /// Within a populated set, every emitted FnDecl was registered as a node,
    /// so a queried key is always present in the graph and absence from
    /// `may_gc` is a real NoGC proof.  A key genuinely unknown to a populated
    /// set (a function emitted from outside the registered universe — none
    /// exist today) is NOT in `may_gc`, so this would report NoGC; such a path
    /// would be unsound, so any future consumer that lowers a not-registered
    /// function MUST treat it as MayGC unconditionally rather than trust this
    /// method (same discipline as `preempt_keep`).
    pub fn is_no_gc(&self, key: &FnKey) -> bool {
        self.populated && !self.may_gc.contains(key)
    }

    /// Whether the given function MAY trigger GC.  Exact complement of
    /// [`is_no_gc`] within a populated set; when not populated, EVERYONE is
    /// MayGC.
    pub fn may_gc(&self, key: &FnKey) -> bool {
        !self.populated || self.may_gc.contains(key)
    }

    /// True when the pre-pass actually ran (non-empty universe).  When the
    /// analysis was never populated, callers MUST treat every function as
    /// MayGC.
    pub fn populated(&self) -> bool {
        self.populated
    }

    /// Number of functions proven MayGC (for diagnostics / CLI summary).
    pub fn may_gc_count(&self) -> usize {
        self.may_gc.len()
    }
}

// Internal flags accumulated per node during the walk.
#[derive(Default, Clone)]
struct NodeFlags {
    /// Body contains a provably-allocating expression (§3.3).
    allocates: bool,
    /// Body makes an indirect / closure / fn-pointer / runtime-dispatched call
    /// (target unknown → cannot prove the target NoGC → seed MayGC).
    makes_indirect: bool,
    /// Body calls an external (FFI) function (its may-GC is unknown → top).
    makes_ffi: bool,
    /// Body calls a name we could NOT resolve to any known FnDecl
    /// (cross-module / closure-local / unknown) → top.
    unresolved_callee: bool,
    /// Direct, resolved call edges to other graph nodes.
    edges: HashSet<FnKey>,
}

impl NodeFlags {
    /// `self_may_gc(node)` (§3.2): the node is MayGC on its own, before
    /// transitive propagation, iff ANY seed fired.
    fn self_may_gc(&self) -> bool {
        self.allocates || self.makes_indirect || self.makes_ffi || self.unresolved_callee
    }
}

/// Render a parameter list to a stable signature string.  Unknown / generic
/// types render as `?`.  Identical to `preempt_keep::param_sig`.
fn param_sig(params: &[Param]) -> String {
    let mut parts: Vec<String> = Vec::with_capacity(params.len());
    for p in params {
        parts.push(typeref_sig(&p.ty));
    }
    parts.join(",")
}

/// Stable string form of a source `TypeRef`, sufficient to distinguish
/// concrete scalar overloads.  Anything we cannot render to a concrete
/// primitive renders to `?`.  Identical to `preempt_keep::typeref_sig`.
fn typeref_sig(t: &TypeRef) -> String {
    match t {
        TypeRef::Readonly(inner, _) | TypeRef::Mut(inner, _) | TypeRef::Unsafe(inner, _) => {
            typeref_sig(inner)
        }
        TypeRef::Named { path, generics, .. } => {
            let name = path.join(".");
            if generics.is_empty() {
                name
            } else {
                format!(
                    "{}<{}>",
                    name,
                    generics.iter().map(typeref_sig).collect::<Vec<_>>().join(",")
                )
            }
        }
        TypeRef::Array(inner, _) => format!("[]{}", typeref_sig(inner)),
        TypeRef::FixedArray(n, inner, _) => format!("[{}]{}", n, typeref_sig(inner)),
        TypeRef::Tuple(elems, _) => {
            format!("({})", elems.iter().map(typeref_sig).collect::<Vec<_>>().join(","))
        }
        TypeRef::Unit(_) => "()".to_string(),
        _ => "?".to_string(),
    }
}

/// Whether a `TypeRef` is a FUNCTION type (`fn(...) -> R`).  Used to discern
/// the allocating method-value `as fn(...)` cast from a non-allocating scalar
/// `as T` cast.  Conservative: only a clearly-rendered non-function type is
/// non-allocating; anything we cannot classify defaults to "function-ish"
/// (allocating).
fn typeref_is_function(t: &TypeRef) -> bool {
    match t {
        TypeRef::Readonly(inner, _) | TypeRef::Mut(inner, _) | TypeRef::Unsafe(inner, _) => {
            typeref_is_function(inner)
        }
        // A plain named scalar / user type, array, fixed-array, tuple, unit,
        // or pointer is NOT a function type → scalar cast (non-allocating).
        TypeRef::Named { .. }
        | TypeRef::Array(..)
        | TypeRef::FixedArray(..)
        | TypeRef::Tuple(..)
        | TypeRef::Unit(_) => false,
        // Function type or anything else (protocol, etc.) → treat as
        // function-shaped → conservatively allocating.
        _ => true,
    }
}

/// Source-syntactic approximation of an expression's TYPE signature for
/// overload disambiguation.  `Some(sig)` only when source-evident; `None` =
/// unknown → edge to all candidates (conservative).  Identical to
/// `preempt_keep::approx_arg_sig_lit`.
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
/// argument signatures.  Identical to `preempt_keep::overload_compatible`.
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
            None => continue,
            Some(a) => {
                if *pp == "?" || pp.contains('<') {
                    continue;
                }
                if pp != a {
                    return false;
                }
            }
        }
    }
    true
}

/// Builder for the call-graph + per-node may-GC flags.
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

    /// Whether a bare name is a known free-fn (so a value-position use is an
    /// address-taken fn reference, NOT a unit-variant allocation).
    fn is_known_free_fn(&self, name: &str) -> bool {
        self.free_by_name.contains_key(name) || self.external_names.contains(name)
    }

    /// Walk a function body, attributing edges / flags to `cur_key`.
    fn analyze_fn(&mut self, key: &FnKey, f: &'a FnDecl) {
        let recv_type = f.receiver.as_ref().map(|r| r.type_name.clone());
        // The function's own parameters (+ receiver `self`) are in scope as
        // non-allocating reads from the body's perspective.
        let mut scope = Scope::new();
        for p in &f.params {
            scope.bind(&p.name);
        }
        match &f.body {
            FnBody::Block(b) => self.walk_block(key, recv_type.as_deref(), &mut scope, b),
            FnBody::Expr(e) => self.walk_expr(key, recv_type.as_deref(), &mut scope, e),
            FnBody::External => {}
        }
    }

    fn walk_block(&mut self, cur: &FnKey, recv: Option<&str>, scope: &mut Scope, b: &Block) {
        // A block introduces a binding sub-scope: lets declared here are
        // visible to later statements in this block but not outside it.
        let mark = scope.enter();
        for s in &b.stmts {
            self.walk_stmt(cur, recv, scope, s);
        }
        if let Some(t) = &b.trailing {
            self.walk_expr(cur, recv, scope, t);
        }
        scope.exit(mark);
    }

    fn walk_stmt(&mut self, cur: &FnKey, recv: Option<&str>, scope: &mut Scope, s: &Stmt) {
        match s {
            Stmt::Let(d) => {
                // RHS is evaluated BEFORE the binding comes into scope.
                self.walk_expr(cur, recv, scope, &d.value);
                bind_pattern(scope, &d.pattern);
            }
            Stmt::Const(c) => {
                self.walk_expr(cur, recv, scope, &c.value);
                scope.bind(&c.name);
            }
            Stmt::Expr(e) => self.walk_expr(cur, recv, scope, e),
            Stmt::Assign { target, value, .. } => {
                self.walk_expr(cur, recv, scope, target);
                self.walk_expr(cur, recv, scope, value);
            }
            Stmt::TupleAssign { lhs, rhs, .. } => {
                for e in lhs {
                    self.walk_expr(cur, recv, scope, e);
                }
                for e in rhs {
                    self.walk_expr(cur, recv, scope, e);
                }
            }
            Stmt::Return { value: Some(v), .. } => self.walk_expr(cur, recv, scope, v),
            Stmt::Return { value: None, .. } => {}
            Stmt::Throw { value, .. } => {
                // A `throw value` may heap-box a value-type payload (F0 survey)
                // → conservatively allocating.
                self.flag_mut(cur).allocates = true;
                self.walk_expr(cur, recv, scope, value);
            }
            Stmt::Defer { body, .. }
            | Stmt::ErrDefer { body, .. }
            | Stmt::OkDefer { body, .. }
            | Stmt::DeferWithResult { body, .. } => self.walk_expr(cur, recv, scope, body),
            Stmt::ConsumeScope { init, body, .. } => {
                self.walk_expr(cur, recv, scope, init);
                self.walk_block(cur, recv, scope, body);
            }
            Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => {
                // Compile-time only — emits no runtime code, hence no alloc;
                // but sub-exprs are walked for edges (defensive — they should
                // carry none).
                self.walk_expr(cur, recv, scope, expr);
            }
            Stmt::Apply { args, .. } => {
                for a in args {
                    self.walk_expr(cur, recv, scope, a);
                }
            }
            Stmt::Calc { steps, .. } => {
                for st in steps {
                    self.walk_expr(cur, recv, scope, &st.expr);
                }
            }
            Stmt::Break(_) | Stmt::Continue(_) | Stmt::Reveal { .. } => {}
        }
    }

    fn walk_expr(&mut self, cur: &FnKey, recv: Option<&str>, scope: &mut Scope, e: &Expr) {
        match &e.kind {
            // ===================== CALLS =====================
            ExprKind::Call { func, args, trailing } => {
                self.handle_call(cur, recv, scope, func, args);
                for a in args {
                    self.walk_expr(cur, recv, scope, a.expr());
                }
                if let Some(t) = trailing {
                    // A trailing block / fn is invoked indirectly by the DSL
                    // callee (target re-invokes the block) → indirect.
                    self.flag_mut(cur).makes_indirect = true;
                    match t {
                        Trailing::Block(b) => self.walk_block(cur, recv, scope, b),
                        Trailing::Fn(fsb) => self.walk_fnbody(cur, recv, scope, &fsb.body),
                        Trailing::LegacyBlockWithParams(tb) => {
                            self.walk_block(cur, recv, scope, &tb.body)
                        }
                    }
                }
            }

            // ===================== NAMES =====================
            ExprKind::Ident(name) => {
                // A bare ident in value position is one of:
                //   (a) a local / param / capture READ — non-allocating;
                //   (b) an address-taken known-fn reference — non-allocating
                //       (taking a fn address does not allocate, §3.2 NB);
                //   (c) a sum UNIT-VARIANT constructor (`Red`, non-NPO `None`)
                //       — ALLOCATES `nova_make_T_V()` (F0 survey);
                //   (d) a module-level const / cross-module global — read,
                //       non-allocating, but indistinguishable from (c) here.
                // SOUND verdict: non-allocating iff it resolves to an in-scope
                // binding or a known fn name; otherwise conservatively
                // ALLOCATING (covers the unit-variant case (c)).
                if !scope.contains(name) && !self.is_known_free_fn(name) {
                    self.flag_mut(cur).allocates = true;
                }
            }
            ExprKind::Path(_) => {
                // `Type.method` / `module.func` used as a VALUE (not a callee).
                // A bare path value could be a qualified unit-variant
                // constructor (`Color.Red`) or a static method reference.
                // Address-taking a fn does not allocate, but a qualified
                // unit-variant does → conservatively ALLOCATING.
                self.flag_mut(cur).allocates = true;
            }
            ExprKind::SelfAccess => {
                // `@field` read of the receiver — `nova_self->field`, no alloc.
            }

            // ===================== ACCESS =====================
            ExprKind::Member { obj, .. } => {
                // Field / `obj.0` read accessor — no alloc; recurse into obj.
                self.walk_expr(cur, recv, scope, obj);
            }
            ExprKind::Index { obj, index } => {
                // A scalar-index element READ is non-allocating; a RANGE index
                // is a slice → allocates a new view/buffer (F0 survey).
                if matches!(index.kind, ExprKind::Range { .. }) {
                    self.flag_mut(cur).allocates = true;
                }
                self.walk_expr(cur, recv, scope, obj);
                self.walk_expr(cur, recv, scope, index);
            }
            ExprKind::TurboFish { base, .. } => self.walk_expr(cur, recv, scope, base),

            // ===================== CONSTRUCTORS (ALLOCATE) =====================
            ExprKind::RecordLit { fields, .. } => {
                // Plain heap-record path allocates; value-record (D226) stack
                // path is non-allocating but discriminating it needs the
                // value_record_names registry + escape analysis (unavailable
                // here) → conservatively ALLOCATING (F0 survey, uncertain).
                self.flag_mut(cur).allocates = true;
                for fld in fields {
                    if let Some(v) = &fld.value {
                        self.walk_expr(cur, recv, scope, v);
                    } else if !scope.contains(&fld.name) && !self.is_known_free_fn(&fld.name) {
                        // shorthand `{ name }` punning a non-binding name —
                        // same unit-variant caveat as a bare Ident.  (The
                        // record itself already allocates, so this is moot, but
                        // kept for symmetry / future refactors.)
                        self.flag_mut(cur).allocates = true;
                    }
                }
            }
            ExprKind::ArrayLit(elems) => {
                self.flag_mut(cur).allocates = true;
                for el in elems {
                    match el {
                        ArrayElem::Item(x) | ArrayElem::Spread(x) => {
                            self.walk_expr(cur, recv, scope, x)
                        }
                    }
                }
            }
            ExprKind::MapLit { elems, .. } => {
                self.flag_mut(cur).allocates = true;
                for el in elems {
                    match el {
                        MapElem::Pair(k, v) => {
                            self.walk_expr(cur, recv, scope, k);
                            self.walk_expr(cur, recv, scope, v);
                        }
                        MapElem::Spread(x) => self.walk_expr(cur, recv, scope, x),
                    }
                }
            }
            ExprKind::TupleLit(elems) => {
                // All-concrete-mono path emits a STACK struct (no alloc), but
                // the legacy/erased path heap-boxes str/Option/nested-tuple/
                // unit elements; which fires is post-mono type-dependent → a
                // sound template-level verdict is ALLOCATING (F0 survey).
                self.flag_mut(cur).allocates = true;
                for x in elems {
                    self.walk_expr(cur, recv, scope, x);
                }
            }
            ExprKind::Range { start, end, .. } => {
                // When the `Range` record is registered, `a..b` allocates a
                // `Nova_Range` (F0 survey, 18934).  Conservatively allocating.
                self.flag_mut(cur).allocates = true;
                if let Some(s) = start {
                    self.walk_expr(cur, recv, scope, s);
                }
                if let Some(en) = end {
                    self.walk_expr(cur, recv, scope, en);
                }
            }
            ExprKind::InterpolatedStr { parts } => {
                // Always allocates a StringBuilder buffer + per-part append.
                self.flag_mut(cur).allocates = true;
                for p in parts {
                    if let InterpStrPart::Expr { expr, .. } = p {
                        self.walk_expr(cur, recv, scope, expr);
                    }
                }
            }
            ExprKind::TaggedTemplate { tag, args, .. } => {
                // Concatenates parts/args into a nova_str buffer → allocates.
                self.flag_mut(cur).allocates = true;
                self.walk_expr(cur, recv, scope, tag);
                for a in args {
                    self.walk_expr(cur, recv, scope, a);
                }
            }

            // ===================== CLOSURES (ALLOCATE env) =====================
            ExprKind::Lambda { params, body, .. } => {
                // Creating a closure heap-allocates its env + clos struct.  The
                // INVOCATION elsewhere is the indirect call; the CREATION here
                // is the allocation.  Both keep the enclosing fn MayGC.
                self.flag_mut(cur).allocates = true;
                let mark = scope.enter();
                for p in params {
                    scope.bind(&p.name);
                }
                self.walk_expr(cur, recv, scope, body);
                scope.exit(mark);
            }
            ExprKind::ClosureLight { params, body } => {
                self.flag_mut(cur).allocates = true;
                let mark = scope.enter();
                for p in params {
                    scope.bind(&p.name);
                }
                match body {
                    ClosureBody::Expr(x) => self.walk_expr(cur, recv, scope, x),
                    ClosureBody::Block(b) => self.walk_block(cur, recv, scope, b),
                }
                scope.exit(mark);
            }
            ExprKind::ClosureFull(fsb) => {
                self.flag_mut(cur).allocates = true;
                let mark = scope.enter();
                for p in &fsb.params {
                    scope.bind(&p.name);
                }
                self.walk_fnbody(cur, recv, scope, &fsb.body);
                scope.exit(mark);
            }

            // ===================== EFFECT / CONCURRENCY (ALLOC + INDIRECT) =====
            ExprKind::With { bindings, body } => {
                // Heap-allocates effect-handler vtable + ctx; handlers invoked
                // indirectly.
                self.flag_mut(cur).allocates = true;
                self.flag_mut(cur).makes_indirect = true;
                for wb in bindings {
                    self.walk_expr(cur, recv, scope, &wb.handler);
                }
                self.walk_block(cur, recv, scope, body);
            }
            ExprKind::HandlerLit { methods, .. } | ExprKind::ProtocolLit { methods, .. } => {
                // Builds a vtable + ctx fat-pointer → allocates.
                self.flag_mut(cur).allocates = true;
                for m in methods {
                    match &m.body {
                        HandlerMethodBody::Expr(x) => self.walk_expr(cur, recv, scope, x),
                        HandlerMethodBody::Block(b) => self.walk_block(cur, recv, scope, b),
                    }
                }
            }
            ExprKind::Spawn(inner) => {
                // SpawnCtx heap-alloc + effect snapshot + scheduler dispatch.
                self.flag_mut(cur).allocates = true;
                self.flag_mut(cur).makes_indirect = true;
                self.walk_expr(cur, recv, scope, inner);
            }
            ExprKind::Supervised { body, cancel } => {
                self.flag_mut(cur).allocates = true;
                self.flag_mut(cur).makes_indirect = true;
                if let Some(c) = cancel {
                    self.walk_expr(cur, recv, scope, c);
                }
                self.walk_block(cur, recv, scope, body);
            }
            ExprKind::Detach(b) | ExprKind::Blocking(b) => {
                // detach → indirect fiber + ctx alloc; blocking → FFI/
                // threadpool offload + ctx alloc.
                self.flag_mut(cur).allocates = true;
                self.flag_mut(cur).makes_indirect = true;
                self.flag_mut(cur).makes_ffi = true;
                self.walk_block(cur, recv, scope, b);
            }
            ExprKind::ParallelFor { pattern, iter, body, .. } => {
                // Desugars to spawn-per-element → ctx allocs + scheduler.
                self.flag_mut(cur).allocates = true;
                self.flag_mut(cur).makes_indirect = true;
                self.walk_expr(cur, recv, scope, iter);
                let mark = scope.enter();
                bind_pattern(scope, pattern);
                self.walk_block(cur, recv, scope, body);
                scope.exit(mark);
            }
            ExprKind::Select { arms } => {
                // Channel-select runtime park/wake → indirect; the runtime path
                // may allocate select state → conservatively allocating.
                self.flag_mut(cur).allocates = true;
                self.flag_mut(cur).makes_indirect = true;
                for arm in arms {
                    let mark = scope.enter();
                    match &arm.op {
                        SelectOp::Recv { chan, binding } => {
                            self.walk_expr(cur, recv, scope, chan);
                            if let Some(name) = binding {
                                scope.bind(name);
                            }
                        }
                        SelectOp::Send { chan, value } => {
                            self.walk_expr(cur, recv, scope, chan);
                            self.walk_expr(cur, recv, scope, value);
                        }
                        SelectOp::Default => {}
                    }
                    if let Some(g) = &arm.guard {
                        self.walk_expr(cur, recv, scope, g);
                    }
                    self.walk_block(cur, recv, scope, &arm.body);
                    scope.exit(mark);
                }
            }

            // ===================== UNWIND / THROW (heap-box payload) ==========
            ExprKind::Throw(inner) => {
                // throw of a non-pointer value type heap-boxes the payload.
                self.flag_mut(cur).allocates = true;
                self.walk_expr(cur, recv, scope, inner);
            }
            ExprKind::Interrupt(opt) => {
                // interrupt with a ValueStruct payload heap-boxes a copy.
                self.flag_mut(cur).allocates = true;
                if let Some(x) = opt {
                    self.walk_expr(cur, recv, scope, x);
                }
            }
            ExprKind::Try(inner) | ExprKind::Bang(inner) => {
                // The success-unwrap path is non-allocating, but the typed-Err
                // propagation path heap-boxes a value-type Err; the payload
                // category is type-dependent → conservatively ALLOCATING.
                self.flag_mut(cur).allocates = true;
                self.walk_expr(cur, recv, scope, inner);
            }

            // ===================== SCALAR / PURE OPS (allowlist) ==============
            ExprKind::Binary { op, left, right } => {
                // str `+` (concat) allocates a buffer; all other scalar / str
                // ops are pure C operators / non-allocating helpers.  We cannot
                // type the operands at the template level, so any `+` could be
                // str-concat → conservatively ALLOCATING (sound).  Other ops
                // are provably non-allocating.
                if matches!(op, BinOp::Add) {
                    self.flag_mut(cur).allocates = true;
                }
                self.walk_expr(cur, recv, scope, left);
                self.walk_expr(cur, recv, scope, right);
            }
            ExprKind::Unary { operand, .. } => {
                // Neg / Not / BitNot / AddrOf / Deref — pure C unary ops.
                // (AddrOf escape-promotion happens at the BINDING site, not
                // here; that site is a `let mut` whose closure-capture already
                // forces MayGC via the capturing Lambda.)
                self.walk_expr(cur, recv, scope, operand);
            }
            ExprKind::As(inner, ty) => {
                // Scalar `as T` cast is a direct C-cast (non-allocating); the
                // method-value `obj.@m as fn(...)` cast builds a closure (alloc).
                if typeref_is_function(ty) {
                    self.flag_mut(cur).allocates = true;
                }
                self.walk_expr(cur, recv, scope, inner);
            }
            ExprKind::Is(inner, _) => {
                // Runtime tag / NPO compare — pure, non-allocating.
                self.walk_expr(cur, recv, scope, inner);
            }
            ExprKind::Coalesce(l, r) => {
                // `??` ternary over already-emitted operands — no alloc itself.
                self.walk_expr(cur, recv, scope, l);
                self.walk_expr(cur, recv, scope, r);
            }

            // ===================== CONTROL FLOW (scaffolding, no alloc) =======
            ExprKind::If { cond, then, else_ } => {
                self.walk_expr(cur, recv, scope, cond);
                self.walk_block(cur, recv, scope, then);
                self.walk_else(cur, recv, scope, else_);
            }
            ExprKind::IfLet { pattern, scrutinee, then, else_ } => {
                self.walk_expr(cur, recv, scope, scrutinee);
                let mark = scope.enter();
                bind_pattern(scope, pattern);
                self.walk_block(cur, recv, scope, then);
                scope.exit(mark);
                self.walk_else(cur, recv, scope, else_);
            }
            ExprKind::Match { scrutinee, arms } => {
                self.walk_expr(cur, recv, scope, scrutinee);
                for arm in arms {
                    let mark = scope.enter();
                    bind_pattern(scope, &arm.pattern);
                    if let Some(g) = &arm.guard {
                        self.walk_expr(cur, recv, scope, g);
                    }
                    match &arm.body {
                        MatchArmBody::Expr(x) => self.walk_expr(cur, recv, scope, x),
                        MatchArmBody::Block(b) => self.walk_block(cur, recv, scope, b),
                    }
                    scope.exit(mark);
                }
            }
            ExprKind::For { pattern, iter, body, invariants, decreases, .. } => {
                self.walk_expr(cur, recv, scope, iter);
                let mark = scope.enter();
                bind_pattern(scope, pattern);
                self.walk_block(cur, recv, scope, body);
                for inv in invariants {
                    self.walk_expr(cur, recv, scope, inv);
                }
                if let Some(d) = decreases {
                    self.walk_expr(cur, recv, scope, d);
                }
                scope.exit(mark);
            }
            ExprKind::While { cond, body, invariants, decreases } => {
                self.walk_expr(cur, recv, scope, cond);
                self.walk_block(cur, recv, scope, body);
                for inv in invariants {
                    self.walk_expr(cur, recv, scope, inv);
                }
                if let Some(d) = decreases {
                    self.walk_expr(cur, recv, scope, d);
                }
            }
            ExprKind::WhileLet { pattern, scrutinee, body, invariants, decreases } => {
                self.walk_expr(cur, recv, scope, scrutinee);
                let mark = scope.enter();
                bind_pattern(scope, pattern);
                self.walk_block(cur, recv, scope, body);
                for inv in invariants {
                    self.walk_expr(cur, recv, scope, inv);
                }
                if let Some(d) = decreases {
                    self.walk_expr(cur, recv, scope, d);
                }
                scope.exit(mark);
            }
            ExprKind::Loop { body, invariants, decreases } => {
                self.walk_block(cur, recv, scope, body);
                for inv in invariants {
                    self.walk_expr(cur, recv, scope, inv);
                }
                if let Some(d) = decreases {
                    self.walk_expr(cur, recv, scope, d);
                }
            }
            ExprKind::Block(b) => self.walk_block(cur, recv, scope, b),
            ExprKind::Forbid { body, .. } | ExprKind::Realtime { body, .. } => {
                self.walk_block(cur, recv, scope, body);
            }
            ExprKind::Forall { var, range, body, .. }
            | ExprKind::Exists { var, range, body, .. } => {
                self.walk_expr(cur, recv, scope, range);
                let mark = scope.enter();
                scope.bind(var);
                self.walk_expr(cur, recv, scope, body);
                scope.exit(mark);
            }

            // ===================== LEAVES (no sub-expr, no alloc) =============
            ExprKind::IntLit(_)
            | ExprKind::FloatLit(_)
            | ExprKind::StrLit(_)
            | ExprKind::BoolLit(_)
            | ExprKind::UnitLit
            | ExprKind::CharLit(_)
            | ExprKind::NullPtrLit => {}
        }
    }

    fn walk_fnbody(&mut self, cur: &FnKey, recv: Option<&str>, scope: &mut Scope, b: &FnBody) {
        match b {
            FnBody::Block(blk) => self.walk_block(cur, recv, scope, blk),
            FnBody::Expr(e) => self.walk_expr(cur, recv, scope, e),
            FnBody::External => {}
        }
    }

    fn walk_else(&mut self, cur: &FnKey, recv: Option<&str>, scope: &mut Scope, else_: &Option<ElseBranch>) {
        match else_ {
            Some(ElseBranch::Block(b)) => self.walk_block(cur, recv, scope, b),
            Some(ElseBranch::If(e)) => self.walk_expr(cur, recv, scope, e),
            None => {}
        }
    }

    // ---------------- overload resolution (mirror preempt_keep) ----------------

    /// Approximate an argument expression's type signature for overload
    /// disambiguation.  Conservative: any uncertainty yields `None`.
    fn approx_arg_sig(&self, e: &Expr) -> Option<String> {
        if let Some(s) = approx_arg_sig_lit(e) {
            return Some(s);
        }
        match &e.kind {
            ExprKind::Call { func, .. } => self.callee_return_sig(func),
            ExprKind::TurboFish { base, .. } => self.approx_arg_sig(base),
            _ => None,
        }
    }

    /// Resolve the concrete return-type signature of a call's `func`, only when
    /// the callee resolves to candidate FnDecls that AGREE on a single concrete
    /// (non-generic) return signature.  Identical logic to
    /// `preempt_keep::callee_return_sig`.
    fn callee_return_sig(&self, func: &Expr) -> Option<String> {
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
            if sig == "?" || sig.contains('?') || sig.contains('<') {
                return None;
            }
            match &agreed {
                None => agreed = Some(sig),
                Some(a) if *a == sig => {}
                Some(_) => return None,
            }
        }
        agreed
    }

    /// Resolve a call's `func` to graph edges / flags on `cur`.
    fn handle_call(&mut self, cur: &FnKey, recv: Option<&str>, scope: &mut Scope, func: &Expr, args: &[CallArg]) {
        let approx: Vec<Option<String>> = args.iter().map(|a| self.approx_arg_sig(a.expr())).collect();
        let arity_certain = args.iter().all(|a| matches!(a, CallArg::Item(_)));

        match &func.kind {
            ExprKind::Member { obj, name } => {
                if matches!(obj.kind, ExprKind::SelfAccess) {
                    // `@name(args)` — self method call; receiver type known.
                    if let Some(rt) = recv {
                        self.resolve_method_edge(cur, rt, name, &approx, arity_certain);
                    } else {
                        self.flag_mut(cur).makes_indirect = true;
                    }
                } else {
                    // Method call on a non-self receiver: receiver type is not
                    // source-evident in a pre-pass → could resolve to any
                    // `*::name` overload → conservatively INDIRECT (top).
                    self.flag_mut(cur).makes_indirect = true;
                    self.walk_expr(cur, recv, scope, obj);
                }
            }
            ExprKind::Ident(name) => {
                if self.external_names.contains(name) {
                    self.flag_mut(cur).makes_ffi = true;
                } else if self.free_by_name.contains_key(name) {
                    self.resolve_free_edge(cur, name, &approx, arity_certain);
                } else {
                    // Unknown bare-ident callee: closure / fn-ptr local OR a
                    // cross-module fn we never saw → unresolved (top).
                    self.flag_mut(cur).unresolved_callee = true;
                }
            }
            ExprKind::Path(parts) => {
                self.handle_path_call(cur, parts, &approx, arity_certain);
            }
            ExprKind::SelfAccess => {
                // `@(...)` — calling the receiver itself as a value → indirect.
                self.flag_mut(cur).makes_indirect = true;
            }
            ExprKind::TurboFish { base, .. } => {
                self.handle_call(cur, recv, scope, base, args);
            }
            _ => {
                // Call result / index / paren-expr callee → indirect (target
                // not a static fn symbol).
                self.flag_mut(cur).makes_indirect = true;
                self.walk_expr(cur, recv, scope, func);
            }
        }
    }

    fn handle_path_call(&mut self, cur: &FnKey, parts: &[String], approx: &[Option<String>], arity_certain: bool) {
        if parts.len() >= 2 {
            let last = &parts[parts.len() - 1];
            let prev = &parts[parts.len() - 2];
            if self.external_methods.contains(&(prev.clone(), last.clone())) {
                self.flag_mut(cur).makes_ffi = true;
                return;
            }
            if self.method_by_name.contains_key(&(prev.clone(), last.clone())) {
                self.resolve_method_edge(cur, prev, last, approx, arity_certain);
                return;
            }
            if self.external_names.contains(last) {
                self.flag_mut(cur).makes_ffi = true;
            } else if self.free_by_name.contains_key(last) {
                self.resolve_free_edge(cur, last, approx, arity_certain);
            } else {
                // Unknown Type.method / module.func → cross-module / unresolved.
                self.flag_mut(cur).unresolved_callee = true;
            }
            return;
        }
        if let Some(name) = parts.first() {
            if self.external_names.contains(name) {
                self.flag_mut(cur).makes_ffi = true;
            } else if self.free_by_name.contains_key(name) {
                self.resolve_free_edge(cur, name, approx, arity_certain);
            } else {
                self.flag_mut(cur).unresolved_callee = true;
            }
        }
    }

    fn resolve_free_edge(&mut self, cur: &FnKey, name: &str, approx: &[Option<String>], arity_certain: bool) {
        let candidates: Vec<FnKey> = match self.free_by_name.get(name) {
            Some(v) => v.iter().map(|(k, _)| k.clone()).collect(),
            None => return,
        };
        self.add_candidate_edges(cur, &candidates, approx, arity_certain);
    }

    fn resolve_method_edge(&mut self, cur: &FnKey, recv_ty: &str, name: &str, approx: &[Option<String>], arity_certain: bool) {
        let candidates: Vec<FnKey> = match self.method_by_name.get(&(recv_ty.to_string(), name.to_string())) {
            Some(v) => v.iter().map(|(k, _)| k.clone()).collect(),
            None => {
                // No known overload on this receiver type → unresolved (top).
                self.flag_mut(cur).unresolved_callee = true;
                return;
            }
        };
        self.add_candidate_edges(cur, &candidates, approx, arity_certain);
    }

    /// Shared overload-edge logic: edge to the subset of `candidates` whose
    /// param-signature is compatible with the approximated args; if arity is
    /// uncertain or no candidate matched, edge to ALL candidates (superset).
    fn add_candidate_edges(&mut self, cur: &FnKey, candidates: &[FnKey], approx: &[Option<String>], arity_certain: bool) {
        if candidates.is_empty() {
            self.flag_mut(cur).unresolved_callee = true;
            return;
        }
        let mut chosen: Vec<FnKey> = Vec::new();
        if arity_certain {
            for k in candidates {
                let psig = key_param_sig(k);
                if overload_compatible(psig, approx) {
                    chosen.push(k.clone());
                }
            }
        }
        if chosen.is_empty() {
            chosen = candidates.to_vec();
        }
        let f = self.flag_mut(cur);
        for k in chosen {
            f.edges.insert(k);
        }
    }
}

// ---------------------------- scope tracking ----------------------------

/// A flat scope stack of in-scope binding names.  `enter`/`exit` give a stack
/// discipline; bindings pushed after a `mark` are popped on `exit(mark)`.
/// Names can shadow freely (duplicates are fine — `contains` only needs
/// membership).
struct Scope {
    names: Vec<String>,
}

impl Scope {
    fn new() -> Self {
        Scope { names: Vec::new() }
    }
    fn enter(&self) -> usize {
        self.names.len()
    }
    fn exit(&mut self, mark: usize) {
        self.names.truncate(mark);
    }
    fn bind(&mut self, name: &str) {
        // `_` wildcard binds nothing meaningful but is harmless to record.
        self.names.push(name.to_string());
    }
    fn contains(&self, name: &str) -> bool {
        self.names.iter().any(|n| n == name)
    }
}

/// Add every binding introduced by a pattern to the scope.  Variant paths
/// (`Some(x)`, `Red`) bind their sub-patterns but the variant NAME itself is a
/// constructor, not a binding (so `Red` with no sub-patterns binds nothing).
fn bind_pattern(scope: &mut Scope, p: &Pattern) {
    match p {
        Pattern::Wildcard(_) | Pattern::Literal(_, _) => {}
        Pattern::Ident { name, .. } => scope.bind(name),
        Pattern::Variant { kind, .. } => match kind {
            VariantPatternKind::Unit => {}
            VariantPatternKind::Tuple { patterns, .. } => {
                for sub in patterns {
                    bind_pattern(scope, sub);
                }
            }
        },
        Pattern::Record { fields, .. } => {
            for f in fields {
                bind_record_pattern_field(scope, f);
            }
        }
        Pattern::Array { elems, .. } => {
            for el in elems {
                match el {
                    ArrayPatternElem::Item(sub) => bind_pattern(scope, sub),
                    ArrayPatternElem::RestBind(name) => scope.bind(name),
                    ArrayPatternElem::Rest => {}
                }
            }
        }
        Pattern::Tuple(elems, _) => {
            for sub in elems {
                bind_pattern(scope, sub);
            }
        }
        Pattern::Binding { name, inner, .. } => {
            scope.bind(name);
            bind_pattern(scope, inner);
        }
        Pattern::Or { alternatives, .. } => {
            // All alternatives bind the same names (spec invariant); binding
            // the first is sufficient, but bind all for robustness.
            for alt in alternatives {
                bind_pattern(scope, alt);
            }
        }
    }
}

fn bind_record_pattern_field(scope: &mut Scope, f: &RecordPatternField) {
    match &f.pattern {
        Some(p) => bind_pattern(scope, p),
        // Field punning `{ name }` binds `name`.
        None => scope.bind(&f.name),
    }
}

// ---------------------------- keys ----------------------------

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

/// Public helper to compute a FnDecl's node key — same scheme as
/// `preempt_keep::fn_key`, so the two analyses can share lookups.
pub fn fn_key(f: &FnDecl) -> FnKey {
    if let Some(recv) = &f.receiver {
        method_key(&recv.type_name, &f.name, &f.params)
    } else {
        free_key(&f.name, &f.params)
    }
}

// ---------------------------- SCC condensation ----------------------------

/// Full Tarjan SCC partition over the edge map.  Returns, for each node, the
/// id of its SCC, plus the list of components (each a `Vec` of node indices)
/// in REVERSE-topological order (Tarjan emits SCCs in reverse-topo, i.e.
/// callees before callers — exactly the order we want for propagation).
///
/// This is the iterative Tarjan from `preempt_keep::cycle_members`, extended
/// to return the COMPLETE partition (every node gets a component id, including
/// singletons) instead of only cycle membership.
struct SccResult {
    /// node index → component id.
    comp_of: Vec<usize>,
    /// component id → member node indices.
    components: Vec<Vec<usize>>,
    /// stable node-index ↔ FnKey mapping.
    nodes: Vec<FnKey>,
    idx_of: HashMap<FnKey, usize>,
}

fn scc_components(flags: &HashMap<FnKey, NodeFlags>) -> SccResult {
    let nodes: Vec<FnKey> = flags.keys().cloned().collect();
    let mut idx_of: HashMap<FnKey, usize> = HashMap::new();
    for (i, n) in nodes.iter().enumerate() {
        idx_of.insert(n.clone(), i);
    }
    let n = nodes.len();

    // Adjacency by index (only edges into known nodes).
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (u, name) in nodes.iter().enumerate() {
        if let Some(nf) = flags.get(name) {
            for e in &nf.edges {
                if let Some(&v) = idx_of.get(e) {
                    adj[u].push(v);
                }
            }
        }
    }

    #[derive(Clone)]
    struct TState {
        index: i64,
        lowlink: i64,
        on_stack: bool,
    }
    let mut state: Vec<TState> = vec![
        TState { index: -1, lowlink: -1, on_stack: false };
        n
    ];
    let mut stack: Vec<usize> = Vec::new();
    let mut counter: i64 = 0;

    let mut comp_of: Vec<usize> = vec![usize::MAX; n];
    let mut components: Vec<Vec<usize>> = Vec::new();

    // Iterative Tarjan. Frame: (node, next-adjacency-cursor).
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
                call_stack.last_mut().unwrap().1 += 1;
                if state[v].index == -1 {
                    call_stack.push((v, 0));
                } else if state[v].on_stack && state[v].index < state[u].lowlink {
                    state[u].lowlink = state[v].index;
                }
            } else {
                // Done with u: if root of SCC, pop the whole component.
                if state[u].lowlink == state[u].index {
                    let comp_id = components.len();
                    let mut comp: Vec<usize> = Vec::new();
                    loop {
                        let w = stack.pop().unwrap();
                        state[w].on_stack = false;
                        comp_of[w] = comp_id;
                        comp.push(w);
                        if w == u {
                            break;
                        }
                    }
                    components.push(comp);
                }
                call_stack.pop();
                if let Some(&(p, _)) = call_stack.last() {
                    if state[u].lowlink < state[p].lowlink {
                        state[p].lowlink = state[u].lowlink;
                    }
                }
            }
        }
    }

    SccResult { comp_of, components, nodes, idx_of }
}

// ---------------------------- driver ----------------------------

/// Compute the may-GC set over the whole program.
///
/// `all_fns` = every FnDecl reachable at codegen time.  Duplicate FnDecls
/// (same item via both `module.items` and a peer file) are harmless —
/// registration is idempotent on the key.
pub fn compute_may_gc_set<'a, I>(all_fns: I) -> MayGcSet
where
    I: IntoIterator<Item = &'a FnDecl>,
{
    let mut an = Analyzer::new();
    let fns: Vec<&'a FnDecl> = all_fns.into_iter().collect();
    for f in &fns {
        an.register(f);
    }
    // Build the work list from the node map so each key is analyzed once with
    // its FnDecl (avoids borrow conflicts during the walk).
    let work: Vec<(FnKey, &'a FnDecl)> =
        an.nodes.iter().map(|(k, f)| (k.clone(), *f)).collect();
    for (key, f) in &work {
        an.analyze_fn(key, f);
    }

    let populated = !an.nodes.is_empty();

    // SCC condensation (callee-before-caller order).
    let scc = scc_components(&an.flags);
    let num_comps = scc.components.len();

    // 1. self-MayGC per component (any member has self_may_gc).
    let mut comp_may_gc: Vec<bool> = vec![false; num_comps];
    for (i, name) in scc.nodes.iter().enumerate() {
        if let Some(nf) = an.flags.get(name) {
            if nf.self_may_gc() {
                comp_may_gc[scc.comp_of[i]] = true;
            }
        }
    }

    // 2. cross-SCC edges: comp_id → set of target comp_ids (excluding self).
    let mut comp_out: Vec<HashSet<usize>> = vec![HashSet::new(); num_comps];
    for (u, name) in scc.nodes.iter().enumerate() {
        let cu = scc.comp_of[u];
        if let Some(nf) = an.flags.get(name) {
            for e in &nf.edges {
                if let Some(&v) = scc.idx_of.get(e) {
                    let cv = scc.comp_of[v];
                    if cv != cu {
                        comp_out[cu].insert(cv);
                    }
                }
            }
        }
    }

    // 3. reverse-topological propagation.  Tarjan emits components in
    //    reverse-topological order (each component appears AFTER all the
    //    components it points to), so iterating `components` in their natural
    //    order processes every callee component before its caller — a single
    //    forward pass suffices, no fixpoint needed.  An SCC is MayGC if any
    //    member self-MayGC OR any out-edge targets a MayGC SCC.
    for cid in 0..num_comps {
        if comp_may_gc[cid] {
            continue;
        }
        for &tgt in &comp_out[cid] {
            if comp_may_gc[tgt] {
                comp_may_gc[cid] = true;
                break;
            }
        }
    }

    // 4. materialize MayGC node set.
    let mut may_gc: HashSet<FnKey> = HashSet::new();
    for (i, name) in scc.nodes.iter().enumerate() {
        if comp_may_gc[scc.comp_of[i]] {
            may_gc.insert(name.clone());
        }
    }

    MayGcSet { may_gc, populated }
}

// =============================================================================
//                                   TESTS
// =============================================================================
#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::*;
    use crate::diag::Span;

    // ---- tiny AST builders (mirror the source syntax shapes) ----

    fn sp() -> Span {
        Span::default()
    }

    fn ty_named(name: &str) -> TypeRef {
        TypeRef::Named {
            path: vec![name.to_string()],
            generics: vec![],
            span: sp(),
        }
    }

    fn e(kind: ExprKind) -> Expr {
        Expr { kind, span: sp() }
    }

    fn int_lit(n: i64) -> Expr {
        e(ExprKind::IntLit(n))
    }

    fn ident(name: &str) -> Expr {
        e(ExprKind::Ident(name.to_string()))
    }

    fn param(name: &str, ty: &str) -> Param {
        Param {
            name: name.to_string(),
            ty: ty_named(ty),
            span: sp(),
            is_variadic: false,
            default: None,
            consume: false,
            is_mut: false,
            is_const: false,
        }
    }

    /// A free fn with an expression body.  Constructed via `Default` spread so
    /// it is robust to FnDecl gaining new fields.
    fn free_fn(name: &str, params: Vec<Param>, body: Expr) -> FnDecl {
        FnDecl {
            name: name.to_string(),
            receiver: None,
            params,
            return_type: Some(ty_named("int")),
            body: FnBody::Expr(body),
            span: sp(),
            ..Default::default()
        }
    }

    /// A free fn with a block body.
    fn free_fn_block(name: &str, params: Vec<Param>, stmts: Vec<Stmt>, trailing: Option<Expr>) -> FnDecl {
        let mut f = free_fn(name, params, int_lit(0));
        f.body = FnBody::Block(Block {
            stmts,
            trailing: trailing.map(Box::new),
            span: sp(),
            is_unsafe: false,
        });
        f
    }

    /// An external (FFI) free fn declaration.
    fn extern_fn(name: &str, params: Vec<Param>) -> FnDecl {
        let mut f = free_fn(name, params, int_lit(0));
        f.is_external = true;
        f.body = FnBody::External;
        f
    }

    /// `f(args)` call expr where `f` is a bare ident.
    fn call_ident(name: &str, args: Vec<Expr>) -> Expr {
        e(ExprKind::Call {
            func: Box::new(ident(name)),
            args: args.into_iter().map(CallArg::Item).collect(),
            trailing: None,
        })
    }

    fn key_of(f: &FnDecl) -> FnKey {
        fn_key(f)
    }

    // ---------------------------- the tests ----------------------------

    #[test]
    fn pure_leaf_is_no_gc() {
        // fn leaf(x int) => x + 1   -- but `+` is conservatively allocating,
        // so use a non-Add pure op for the pure-leaf case.
        let leaf = free_fn(
            "leaf",
            vec![param("x", "int")],
            e(ExprKind::Binary {
                op: BinOp::Sub,
                left: Box::new(ident("x")),
                right: Box::new(int_lit(1)),
            }),
        );
        let fns = vec![&leaf];
        let set = compute_may_gc_set(fns.into_iter().map(|f| f as &FnDecl));
        assert!(set.populated());
        assert!(set.is_no_gc(&key_of(&leaf)), "pure scalar leaf must be NoGC");
    }

    #[test]
    fn nogc_forwarder_is_no_gc() {
        // fn leaf(x int) => x - 1     (NoGC)
        // fn fwd(x int)  => leaf(x)   (calls only NoGC → NoGC)
        let leaf = free_fn(
            "leaf",
            vec![param("x", "int")],
            e(ExprKind::Binary {
                op: BinOp::Sub,
                left: Box::new(ident("x")),
                right: Box::new(int_lit(1)),
            }),
        );
        let fwd = free_fn("fwd", vec![param("x", "int")], call_ident("leaf", vec![ident("x")]));
        let fns: Vec<&FnDecl> = vec![&leaf, &fwd];
        let set = compute_may_gc_set(fns.into_iter());
        assert!(set.is_no_gc(&key_of(&leaf)));
        assert!(set.is_no_gc(&key_of(&fwd)), "forwarder to NoGC must be NoGC");
    }

    #[test]
    fn pure_mutual_recursion_scc_is_no_gc() {
        // fn ping(x int) => pong(x)
        // fn pong(x int) => ping(x)
        // Pure mutual recursion, no allocation → whole SCC NoGC.
        let ping = free_fn("ping", vec![param("x", "int")], call_ident("pong", vec![ident("x")]));
        let pong = free_fn("pong", vec![param("x", "int")], call_ident("ping", vec![ident("x")]));
        let fns: Vec<&FnDecl> = vec![&ping, &pong];
        let set = compute_may_gc_set(fns.into_iter());
        assert!(set.is_no_gc(&key_of(&ping)), "pure recursive SCC must be NoGC");
        assert!(set.is_no_gc(&key_of(&pong)), "pure recursive SCC must be NoGC");
    }

    #[test]
    fn allocator_in_callee_propagates_caller_is_may_gc() {
        // fn allocs(x int) => [x]          (ArrayLit → allocates → MayGC)
        // fn caller(x int) => allocs(x)    (calls MayGC → MayGC)
        let allocs = free_fn(
            "allocs",
            vec![param("x", "int")],
            e(ExprKind::ArrayLit(vec![ArrayElem::Item(ident("x"))])),
        );
        let caller = free_fn("caller", vec![param("x", "int")], call_ident("allocs", vec![ident("x")]));
        let fns: Vec<&FnDecl> = vec![&allocs, &caller];
        let set = compute_may_gc_set(fns.into_iter());
        assert!(!set.is_no_gc(&key_of(&allocs)), "allocating fn must be MayGC");
        assert!(!set.is_no_gc(&key_of(&caller)), "caller of allocating fn must be MayGC");
        assert!(set.may_gc(&key_of(&caller)));
    }

    #[test]
    fn indirect_call_via_closure_param_is_may_gc() {
        // fn higher(f int) => f(1)
        // `f` is a param (in scope) used as a CALLEE → unknown target → MayGC.
        let higher = free_fn_block(
            "higher",
            vec![param("f", "int")],
            vec![Stmt::Expr(call_ident("f", vec![int_lit(1)]))],
            None,
        );
        let fns: Vec<&FnDecl> = vec![&higher];
        let set = compute_may_gc_set(fns.into_iter());
        assert!(!set.is_no_gc(&key_of(&higher)), "indirect call must be MayGC");
    }

    #[test]
    fn ffi_extern_call_is_may_gc() {
        // extern fn c_thing(x int)
        // fn uses_ffi(x int) => c_thing(x)
        let cthing = extern_fn("c_thing", vec![param("x", "int")]);
        let uses_ffi = free_fn("uses_ffi", vec![param("x", "int")], call_ident("c_thing", vec![ident("x")]));
        let fns: Vec<&FnDecl> = vec![&cthing, &uses_ffi];
        let set = compute_may_gc_set(fns.into_iter());
        // c_thing is external → not a node; uses_ffi calls it → makes_ffi → MayGC.
        assert!(!set.is_no_gc(&key_of(&uses_ffi)), "FFI caller must be MayGC");
    }

    #[test]
    fn unresolved_callee_is_may_gc() {
        // fn caller(x int) => nonexistent(x)
        // `nonexistent` is neither a node nor an extern → unresolved → MayGC.
        let caller = free_fn("caller", vec![param("x", "int")], call_ident("nonexistent", vec![ident("x")]));
        let fns: Vec<&FnDecl> = vec![&caller];
        let set = compute_may_gc_set(fns.into_iter());
        assert!(!set.is_no_gc(&key_of(&caller)), "unresolved callee must be MayGC");
    }

    #[test]
    fn recursive_scc_with_one_allocating_member_is_all_may_gc() {
        // fn a(x int) => { allocs_here(); b(x) }   where a allocates an array
        // fn b(x int) => a(x)
        // a allocates → whole {a,b} SCC MayGC.
        let a = free_fn_block(
            "a",
            vec![param("x", "int")],
            vec![Stmt::Expr(e(ExprKind::ArrayLit(vec![ArrayElem::Item(int_lit(0))])))],
            Some(call_ident("b", vec![ident("x")])),
        );
        let b = free_fn("b", vec![param("x", "int")], call_ident("a", vec![ident("x")]));
        let fns: Vec<&FnDecl> = vec![&a, &b];
        let set = compute_may_gc_set(fns.into_iter());
        assert!(!set.is_no_gc(&key_of(&a)), "allocating SCC member must be MayGC");
        assert!(!set.is_no_gc(&key_of(&b)), "whole SCC must be MayGC when one member allocates");
    }

    #[test]
    fn unpopulated_set_makes_everyone_may_gc() {
        let set = MayGcSet::default();
        assert!(!set.populated());
        assert!(!set.is_no_gc(&"fn::anything::".to_string()), "unpopulated → no one is NoGC");
        assert!(set.may_gc(&"fn::anything::".to_string()), "unpopulated → everyone MayGC");
    }

    #[test]
    fn record_literal_allocates() {
        // fn mk() => User { id: 1 }
        let mk = free_fn(
            "mk",
            vec![],
            e(ExprKind::RecordLit {
                type_name: Some(vec!["User".to_string()]),
                fields: vec![RecordLitField {
                    name: "id".to_string(),
                    value: Some(int_lit(1)),
                    is_spread: false,
                    at_shorthand: false,
                    span: sp(),
                }],
                inferred_map_v: None,
            }),
        );
        let fns: Vec<&FnDecl> = vec![&mk];
        let set = compute_may_gc_set(fns.into_iter());
        assert!(!set.is_no_gc(&key_of(&mk)), "record literal allocates → MayGC");
    }

    #[test]
    fn str_concat_add_is_conservatively_may_gc() {
        // fn cat(a int, b int) => a + b
        // `+` is conservatively allocating (could be str-concat at template lvl).
        let cat = free_fn(
            "cat",
            vec![param("a", "int"), param("b", "int")],
            e(ExprKind::Binary {
                op: BinOp::Add,
                left: Box::new(ident("a")),
                right: Box::new(ident("b")),
            }),
        );
        let fns: Vec<&FnDecl> = vec![&cat];
        let set = compute_may_gc_set(fns.into_iter());
        assert!(!set.is_no_gc(&key_of(&cat)), "`+` is conservatively allocating");
    }

    #[test]
    fn unit_variant_bare_ident_is_conservatively_may_gc() {
        // fn red() => Red    (bare ident, not a binding, not a known fn →
        //                     could be a non-NPO unit-variant ctor → allocates)
        let red = free_fn("red", vec![], ident("Red"));
        let fns: Vec<&FnDecl> = vec![&red];
        let set = compute_may_gc_set(fns.into_iter());
        assert!(!set.is_no_gc(&key_of(&red)), "unresolved bare ident (unit-variant) is MayGC");
    }

    #[test]
    fn closure_creation_allocates_caller() {
        // fn make_clo() => { ro f = |x| x ; 0 }
        // closure-light creation allocates env → MayGC.
        let f = free_fn_block(
            "make_clo",
            vec![],
            vec![Stmt::Let(LetDecl {
                mutable: false,
                pattern: Pattern::Ident { name: "f".to_string(), span: sp(), is_mut: false },
                ty: None,
                value: e(ExprKind::ClosureLight {
                    params: vec![ClosureLightParam { name: "x".to_string(), span: sp() }],
                    body: ClosureBody::Expr(Box::new(ident("x"))),
                }),
                span: sp(),
                is_ghost: false,
                consume: false,
            })],
            Some(int_lit(0)),
        );
        let fns: Vec<&FnDecl> = vec![&f];
        let set = compute_may_gc_set(fns.into_iter());
        assert!(!set.is_no_gc(&key_of(&f)), "closure creation allocates → MayGC");
    }

    #[test]
    fn local_binding_read_does_not_allocate() {
        // fn read_local() => { ro y = 3 ; y }
        // `y` is an in-scope binding → its read is NOT a unit-variant → NoGC.
        let f = free_fn_block(
            "read_local",
            vec![],
            vec![Stmt::Let(LetDecl {
                mutable: false,
                pattern: Pattern::Ident { name: "y".to_string(), span: sp(), is_mut: false },
                ty: None,
                value: int_lit(3),
                span: sp(),
                is_ghost: false,
                consume: false,
            })],
            Some(ident("y")),
        );
        let fns: Vec<&FnDecl> = vec![&f];
        let set = compute_may_gc_set(fns.into_iter());
        assert!(set.is_no_gc(&key_of(&f)), "reading an in-scope local must stay NoGC");
    }

    #[test]
    fn deep_nogc_chain_propagates() {
        // a -> b -> c -> leaf, all pure → all NoGC.
        let leaf = free_fn(
            "leaf",
            vec![param("x", "int")],
            e(ExprKind::Binary {
                op: BinOp::Mul,
                left: Box::new(ident("x")),
                right: Box::new(int_lit(2)),
            }),
        );
        let c = free_fn("c", vec![param("x", "int")], call_ident("leaf", vec![ident("x")]));
        let b = free_fn("b", vec![param("x", "int")], call_ident("c", vec![ident("x")]));
        let a = free_fn("a", vec![param("x", "int")], call_ident("b", vec![ident("x")]));
        let fns: Vec<&FnDecl> = vec![&leaf, &c, &b, &a];
        let set = compute_may_gc_set(fns.into_iter());
        assert!(set.is_no_gc(&key_of(&leaf)));
        assert!(set.is_no_gc(&key_of(&c)));
        assert!(set.is_no_gc(&key_of(&b)));
        assert!(set.is_no_gc(&key_of(&a)), "deep pure chain must stay NoGC end-to-end");
    }

    #[test]
    fn may_gc_does_not_leak_to_unrelated_sibling() {
        // alloc() allocates (MayGC); pure() is independent and pure (NoGC).
        // Propagation must NOT flood unrelated nodes.
        let alloc = free_fn("alloc", vec![], e(ExprKind::ArrayLit(vec![])));
        let pure = free_fn(
            "pure",
            vec![param("x", "int")],
            e(ExprKind::Binary {
                op: BinOp::Sub,
                left: Box::new(ident("x")),
                right: Box::new(int_lit(1)),
            }),
        );
        let fns: Vec<&FnDecl> = vec![&alloc, &pure];
        let set = compute_may_gc_set(fns.into_iter());
        assert!(!set.is_no_gc(&key_of(&alloc)));
        assert!(set.is_no_gc(&key_of(&pure)), "unrelated pure sibling must stay NoGC");
    }
}
