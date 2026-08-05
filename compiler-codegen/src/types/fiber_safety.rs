// Plan 238 Ф.1 (D446 "Ф.8-НОВАЯ", решение владельца 2026-08-05): total
// per-function/method safety TAG over the RESOLVED call graph. Phase 1
// scope ONLY — measurement/dump, NO enforcement (that is Ф.2), NO
// diagnostics, nothing wired into `emit_c` (§0/196 — checker-channel-only,
// codegen untouched). See `docs/plans/238-fiber-memory-model.md`, section
// "Ф.8-НОВАЯ — ДЕЙСТВУЮЩАЯ РЕДАКЦИЯ" for the four positions (П.1-П.4) this
// module implements, and `docs/plans/wip/PROGRESS-p238-f1.md` for the
// measured numbers and every documented design/scope decision below.
//
// Entry point: [`run`], called once per `check_module_impl` (types/mod.rs),
// right after `resolved_callees` is finalized. Purely additive: builds an
// in-memory map and, iff `NOVA_DEBUG_FIBER_SAFETY=1`, prints counters to
// STDERR. Nothing here is read by any existing diagnostic path or by
// codegen — behaviorally neutral by construction (the phase-1 "Нейтральность"
// requirement).
//
// ── П.4: graph, not names ──────────────────────────────────────────────
// The graph is built over `resolved_callees` (`ExprId` → callee `FnDecl`
// declaration `Span`) — the SAME channel §0/196 already threads through the
// checker for method mangling. This is deliberately NOT
// `own_fiber_call_names_of_fn` (the `#thread_affine` closure's adjacency
// builder, `types/mod.rs`): that walker matches bare `ExprKind::Ident` call
// targets ONLY and never reads `resolved_callees`, so a method call
// (`table.bump()`) is invisible to it (D446 П.4, measured in
// `PROGRESS-p238-review.md` §E — "методы вне периметра"). `resolved_callees`
// already resolves methods; building the adjacency over it closes that gap
// without touching the thread-affine machinery at all.
//
// ── П.1/П.2: total tag, default flipped ────────────────────────────────
// Every `Item::Fn` (free fn AND method — both receiver kinds; Nova has no
// separate impl-block AST, methods are flat `Item::Fn` with `receiver:
// Some(_)`) gets exactly one [`FnSafety`]. Undecided is a first-class,
// terminal outcome — not a fallback that silently becomes Safe.
//
// ── П.3: basis, and the documented phase-1 approximations ─────────────
// (a) "does not touch reachable mutable state" — a syntactic touch is a
//     direct write to `@field`/a `mut`-parameter place, OR a call whose
//     RESOLVED receiver method is declared `mut @method` (`Receiver::
//     mutable`) on such a place. Purely LOCAL `mut` accumulators (never
//     reaching `@`/a `mut` param) are NOT touches — they cannot be observed
//     from outside this one call, matching the census's own field_write-
//     vs-local_only split (`PROGRESS-p238-review.md` §A) and this phase's
//     job (a per-function property, not the full Ф.7 10-transition
//     escape-grammar, which is Ф.2's enforcement concern).
// (b) "touches under a live lock" — a lightweight, SELF-CONTAINED liveness
//     tracker for `consume`-bound locals initialized from a method call
//     (the `consume g = @mutex.lock()` shape, generalized: ANY consume-
//     bound call-result, not hardcoded to a `Mutex`-named type — matches
//     "no type-name hardcoding" doctrine). This is a DELIBERATELY SMALLER,
//     dedicated approximation of the existing D131 `VarState` flow — not
//     literal code reuse: the existing `ConsumeCtx` pass is deeply
//     entangled with its own diagnostics (E_CONSUME_*) and branch-merge
//     precision (`MaybeConsumed`); wiring THIS analysis through it would
//     require understanding/touching a ~1000-line pass for an unrelated
//     purpose. The shared IDEA (states over a linear resource) is reused;
//     the code is not. Sequential, no branch-merge: a guard is "live" from
//     its declaring `Let` to the first call made directly on that name
//     (approximates `.unlock()`) or its enclosing block's exit.
// (c) "type declared safe (#share), verified structurally" — reuses
//     `protocols::share_check::is_mut_alias_safe` VERBATIM (D415's existing
//     structural walk), applied to the precise reached field's declared
//     type (not the whole receiver type — `MetricsRegistry`'s `mutex`
//     field being `#share` must not need `counters` to also qualify).
//
// ── Combinator inference ("RESOLVED 2026-08-05") ───────────────────────
// [`compute_guarded_params`] is Pass A: for every fn, which of ITS OWN
// function-typed parameters get invoked, directly by name, while one of
// its OWN consume-guards is live — the `Mutex.with_lock`/`RwLock.
// with_read`/`with_write`/`Semaphore.with_permit` shape
// (`std/src/runtime/sync.nv`). Pass B (the main walk) then treats a
// closure LITERAL passed at such a parameter position as running under a
// SYNTHETIC live guard — not the caller's own ambient guard state (the
// caller usually holds none at all; the guard is `with_lock`'s own,
// acquired internally, and is live at the closure's ACTUAL invocation
// time, not at the textual call-site in the caller's body).
//
// ── Generics / cross-module scope (documented, not silently dropped) ──
// A fn with ANY generic parameter is Undecided("generic") — the BROAD
// rule (not bound-sensitive), matching the census methodology exactly so
// the phase-1 dump is comparable to the pre-implementation measurement
// (`PROGRESS-p238-review.md` §A: 19.0% std / 6.3% polaris / 7.4% flagship).
// Bound-sensitivity (only a generic fn that actually sends its type-param
// into parallelism should be Undecided) is explicitly future work — see
// the plan's own "Цена меньше измеренной... работа первой волны" note.
//
// A resolved call whose target `Span` is NOT one of THIS module's own
// `Item::Fn` (i.e. std called from `nova-polaris`, etc.) is
// Undecided("cross_module_callee") — phase 1 measures one compile-unit's
// `Module` at a time (mirrors how `nova check std/src` and `nova check
// nova-polaris/src` are already run separately), so it cannot chase a tag
// across that boundary. This is a REAL, measured, reportable cost — not a
// silent gap — see the PROGRESS doc's numbers.

use std::collections::{HashMap, HashSet};

use crate::ast::{
    Block, CallArg, ClosureBody, ElseBranch, Expr, ExprId, ExprKind, FnBody, FnDecl,
    FnSigBody, Item, MatchArmBody, Module, Param, Pattern, RecordField, Stmt, Trailing,
    TypeDecl, TypeDeclKind, TypeRef,
};
use crate::diag::Span;

/// Stable per-fn/method identity — its declaration span. Matches
/// `resolved_callees`'s Span TARGET (`fd.span`) — the same identity
/// convention `check_unsafe_context_in_module`'s `unsafe_decl_spans`
/// already uses for exactly this reason (methods have no unique name).
pub type FnId = Span;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tag {
    Safe,
    Undecided,
    Unsafe,
}

impl Tag {
    fn rank(self) -> u8 {
        match self {
            Tag::Safe => 0,
            Tag::Undecided => 1,
            Tag::Unsafe => 2,
        }
    }
}

/// П.3 basis + debug-dump vocabulary. `reason` is a FIXED set of static
/// codes (see the dump in [`run`]) so counters can be grouped; `detail`
/// carries the offending callee's rendered name for the two
/// graph-propagated reasons (cheap breadcrumb for a later phase's
/// diagnostic — unused by phase 1 itself).
#[derive(Clone, Debug)]
pub struct FnSafety {
    pub tag: Tag,
    pub reason: &'static str,
    pub detail: Option<String>,
}

// ── Pass 0: flat index over Item::Fn (free fns AND methods) ───────────

fn build_fn_index(module: &Module) -> HashMap<Span, &FnDecl> {
    let mut idx = HashMap::new();
    for item in &module.items {
        if let Item::Fn(fd) = item {
            idx.insert(fd.span, fd);
        }
    }
    idx
}

fn build_type_decls(module: &Module) -> HashMap<String, &TypeDecl> {
    // Mirrors `CapabilityCtx::build`'s merge exactly (types/mod.rs): module-
    // declared types first, registry-only builtin modules (sync.nv's
    // Mutex/RwLock/Semaphore/... among them) fill in via `or_insert` so a
    // module-declared type of the same name wins.
    let mut type_decls: HashMap<String, &TypeDecl> = HashMap::new();
    for item in &module.items {
        if let Item::Type(t) = item {
            type_decls.insert(t.name.clone(), t);
        }
    }
    for ext_mod in crate::codegen::external_registry::builtin_sig_modules() {
        for item in &ext_mod.items {
            if let Item::Type(td) = item {
                type_decls.entry(td.name.clone()).or_insert(td);
            }
        }
    }
    type_decls
}

fn is_func_type(ty: &TypeRef) -> bool {
    match ty {
        TypeRef::Func { .. } => true,
        TypeRef::Readonly(inner, _) | TypeRef::Mut(inner, _) | TypeRef::Uninit(inner, _) => {
            is_func_type(inner)
        }
        _ => false,
    }
}

/// П.2/signature-level Undecided — computed BEFORE looking at the body at
/// all (matches the census methodology's own "decide from the signature
/// first" order, `PROGRESS-p238-review.md` §A).
fn signature_undecided_reason(fd: &FnDecl) -> Option<&'static str> {
    if matches!(fd.body, FnBody::External) {
        return Some("no_body");
    }
    if !fd.generics.is_empty() {
        return Some("generic");
    }
    if fd.is_external || fd.extern_abi.is_some() {
        return Some("extern");
    }
    if fd.params.iter().any(|p| is_func_type(&p.ty)) {
        return Some("fn_param_in_sig");
    }
    None
}

// ── Pass A ("RESOLVED 2026-08-05"): guarded closure parameters ────────
//
// Per fn, which of ITS OWN function-typed parameters (by index) are
// invoked, directly by name, while one of its OWN consume-guards is live.
// Purely intra-procedural — computed once for every fn BEFORE Pass B
// (mirrors `thread_affine_closure`'s "pre-pass, computed once" timing).
//
// Documented coverage limit: only the literal single-hop shape — `consume
// g = <expr>.<method>(); ...; <param>()` (a bare `Ident` call of the exact
// parameter name) while a guard is on the stack. A combinator that calls
// its closure parameter through another layer of indirection is not
// recognised (out of phase-1 scope; under-recognition only makes MORE
// closures fail to be recognised as guarded, which biases toward
// Undecided/Unsafe, never toward a false Safe).

fn compute_guarded_params(module: &Module) -> HashMap<Span, HashSet<usize>> {
    let mut out = HashMap::new();
    for item in &module.items {
        if let Item::Fn(fd) = item {
            if fd.params.is_empty() {
                continue;
            }
            let param_index: HashMap<&str, usize> = fd
                .params
                .iter()
                .enumerate()
                .map(|(i, p)| (p.name.as_str(), i))
                .collect();
            let mut guards: Vec<String> = Vec::new();
            let mut found: HashSet<usize> = HashSet::new();
            match &fd.body {
                FnBody::Block(b) => gp_walk_block(b, &param_index, &mut guards, &mut found),
                FnBody::Expr(e) => gp_walk_expr(e, &param_index, &mut guards, &mut found),
                FnBody::External => {}
            }
            if !found.is_empty() {
                out.insert(fd.span, found);
            }
        }
    }
    out
}

fn gp_walk_block(
    b: &Block,
    params: &HashMap<&str, usize>,
    guards: &mut Vec<String>,
    found: &mut HashSet<usize>,
) {
    let mark = guards.len();
    for s in &b.stmts {
        gp_walk_stmt(s, params, guards, found);
    }
    if let Some(t) = &b.trailing {
        gp_walk_expr(t, params, guards, found);
    }
    guards.truncate(mark);
}

fn gp_walk_stmt(
    s: &Stmt,
    params: &HashMap<&str, usize>,
    guards: &mut Vec<String>,
    found: &mut HashSet<usize>,
) {
    match s {
        Stmt::Let(d) => {
            gp_walk_expr(&d.value, params, guards, found);
            if d.consume {
                if let Pattern::Ident { name, .. } = &d.pattern {
                    if matches!(d.value.kind, ExprKind::Call { .. }) {
                        guards.push(name.clone());
                    }
                }
            }
        }
        Stmt::Expr(e) => gp_walk_expr(e, params, guards, found),
        Stmt::Assign { target, value, .. } => {
            gp_walk_expr(target, params, guards, found);
            gp_walk_expr(value, params, guards, found);
        }
        Stmt::TupleAssign { lhs, rhs, .. } => {
            for e in lhs {
                gp_walk_expr(e, params, guards, found);
            }
            for e in rhs {
                gp_walk_expr(e, params, guards, found);
            }
        }
        Stmt::Return { value, .. } => {
            if let Some(v) = value {
                gp_walk_expr(v, params, guards, found);
            }
        }
        Stmt::Throw { value, .. } => gp_walk_expr(value, params, guards, found),
        Stmt::Defer { body, .. } => gp_walk_expr(body, params, guards, found),
        Stmt::ConsumeScope { init, body, .. } => {
            gp_walk_expr(init, params, guards, found);
            gp_walk_block(body, params, guards, found);
        }
        Stmt::Const(_)
        | Stmt::Break(_)
        | Stmt::Continue(_)
        | Stmt::AssertStatic { .. }
        | Stmt::Assume { .. }
        | Stmt::Apply { .. }
        | Stmt::Calc { .. }
        | Stmt::Reveal { .. } => {}
    }
}

fn gp_walk_expr(
    e: &Expr,
    params: &HashMap<&str, usize>,
    guards: &mut Vec<String>,
    found: &mut HashSet<usize>,
) {
    match &e.kind {
        ExprKind::Call { func, args, trailing } => {
            // The `with_lock`/`with_read`/`with_write`/`with_permit` shape:
            // a bare call of one of THIS fn's own parameters, while a guard
            // introduced earlier in this same body is still live.
            if let ExprKind::Ident(name) = &func.kind {
                if let Some(&idx) = params.get(name.as_str()) {
                    if !guards.is_empty() {
                        found.insert(idx);
                    }
                }
            }
            gp_walk_expr(func, params, guards, found);
            for a in args {
                gp_walk_expr(a.expr(), params, guards, found);
            }
            if let Some(t) = trailing {
                match t {
                    Trailing::Block(b) => gp_walk_block(b, params, guards, found),
                    Trailing::LegacyBlockWithParams(tb) => {
                        gp_walk_block(&tb.body, params, guards, found)
                    }
                    // Trailing closure-sugar — own scope, skip (mirrors
                    // `own_fiber_call_names_expr`'s stance).
                    Trailing::Fn(_) => {}
                }
            }
            // A method called directly ON a live guard name ends its
            // liveness — approximates D131's Consumed transition (`.unlock()`
            // is the common case; ANY call on the exact binding is treated
            // as consuming it, conservative in the SAFE direction: this can
            // only under-count liveness, never over-count it).
            if let ExprKind::Member { obj, .. } = &func.kind {
                if let ExprKind::Ident(name) = &obj.kind {
                    guards.retain(|g| g != name);
                }
            }
        }
        ExprKind::Spawn(_) | ExprKind::Detach(_) | ExprKind::Blocking(_) => {}
        ExprKind::ParallelFor { iter, .. } => gp_walk_expr(iter, params, guards, found),
        ExprKind::Block(b) => gp_walk_block(b, params, guards, found),
        ExprKind::If { cond, then, else_ } => {
            gp_walk_expr(cond, params, guards, found);
            gp_walk_block(then, params, guards, found);
            match else_ {
                Some(ElseBranch::Block(b)) => gp_walk_block(b, params, guards, found),
                Some(ElseBranch::If(e2)) => gp_walk_expr(e2, params, guards, found),
                None => {}
            }
        }
        ExprKind::IfLet { scrutinee, guard, then, else_, .. } => {
            gp_walk_expr(scrutinee, params, guards, found);
            if let Some(g) = guard {
                gp_walk_expr(g, params, guards, found);
            }
            gp_walk_block(then, params, guards, found);
            match else_ {
                Some(ElseBranch::Block(b)) => gp_walk_block(b, params, guards, found),
                Some(ElseBranch::If(e2)) => gp_walk_expr(e2, params, guards, found),
                None => {}
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            gp_walk_expr(scrutinee, params, guards, found);
            for a in arms {
                if let Some(g) = &a.guard {
                    gp_walk_expr(g, params, guards, found);
                }
                match &a.body {
                    MatchArmBody::Expr(be) => gp_walk_expr(be, params, guards, found),
                    MatchArmBody::Block(bb) => gp_walk_block(bb, params, guards, found),
                }
            }
        }
        ExprKind::For { iter, body, .. } => {
            gp_walk_expr(iter, params, guards, found);
            gp_walk_block(body, params, guards, found);
        }
        ExprKind::While { cond, body, .. } => {
            gp_walk_expr(cond, params, guards, found);
            gp_walk_block(body, params, guards, found);
        }
        ExprKind::WhileLet { scrutinee, guard, body, .. } => {
            gp_walk_expr(scrutinee, params, guards, found);
            if let Some(g) = guard {
                gp_walk_expr(g, params, guards, found);
            }
            gp_walk_block(body, params, guards, found);
        }
        ExprKind::Loop { body, .. } => gp_walk_block(body, params, guards, found),
        ExprKind::Supervised { body, cancel, deadline } => {
            if let Some(c) = cancel {
                gp_walk_expr(c, params, guards, found);
            }
            if let Some(dl) = deadline {
                gp_walk_expr(&dl.expr, params, guards, found);
            }
            gp_walk_block(body, params, guards, found);
        }
        ExprKind::With { body, .. } => gp_walk_block(body, params, guards, found),
        ExprKind::Forbid { body, .. } | ExprKind::Realtime { body, .. } => {
            gp_walk_block(body, params, guards, found)
        }
        ExprKind::Try(inner) | ExprKind::Bang(inner) | ExprKind::RefArg(inner) => {
            gp_walk_expr(inner, params, guards, found)
        }
        ExprKind::Coalesce(a, b) => {
            gp_walk_expr(a, params, guards, found);
            gp_walk_expr(b, params, guards, found);
        }
        ExprKind::Binary { left, right, .. } => {
            gp_walk_expr(left, params, guards, found);
            gp_walk_expr(right, params, guards, found);
        }
        ExprKind::Unary { operand, .. } => gp_walk_expr(operand, params, guards, found),
        ExprKind::Member { obj, .. } => gp_walk_expr(obj, params, guards, found),
        ExprKind::Index { obj, index } => {
            gp_walk_expr(obj, params, guards, found);
            gp_walk_expr(index, params, guards, found);
        }
        _ => {}
    }
}

// ── Pass B: per-fn facts (touches + local call-graph edges) ───────────

/// Where a "touch" (a write, or a call to a `mut @method`) is rooted.
/// Only these two shapes are "reachable from outside this one call" —
/// D446 П.3(a)'s "reachable" — a bare local never escapes on its own.
#[derive(Clone, Debug)]
enum PlaceRoot {
    SelfBare,
    SelfField(String),
    MutParam(String),
}

fn place_root(e: &Expr, mut_params: &HashSet<String>) -> Option<PlaceRoot> {
    match &e.kind {
        ExprKind::SelfAccess => Some(PlaceRoot::SelfBare),
        ExprKind::Ident(name) if mut_params.contains(name) => {
            Some(PlaceRoot::MutParam(name.clone()))
        }
        ExprKind::Member { obj, name } => match place_root(obj, mut_params) {
            Some(PlaceRoot::SelfBare) => Some(PlaceRoot::SelfField(name.clone())),
            // Deeper hop: keep the FIRST-hop classification (documented
            // phase-1 approximation — `share_check`'s own recursive walk on
            // the first field's type is still a sound, if imprecise,
            // conservative check for anything reachable beneath it).
            other @ Some(_) => other,
            None => None,
        },
        ExprKind::Index { obj, .. } => place_root(obj, mut_params),
        ExprKind::TurboFish { base, .. }
        | ExprKind::Try(base)
        | ExprKind::Bang(base)
        | ExprKind::RefArg(base) => place_root(base, mut_params),
        _ => None,
    }
}

#[derive(Default)]
struct FnFacts {
    /// Resolved LOCAL callees (target span found in this module's own
    /// `fn_index`) reached anywhere in this fn's synchronous execution,
    /// INCLUDING nested closure-literal bodies it constructs (a deliberate
    /// OVER-approximation — see module doc "touches inside nested closures
    /// count toward the enclosing fn").
    local_callees: HashSet<Span>,
    /// A touch (write or `mut @method` call) rooted at `@`/a `mut` param.
    touches: Vec<Touch>,
    /// At least one resolved call whose target is outside THIS module's
    /// own `fn_index` — see module doc "cross-module scope".
    cross_module_callee: bool,
}

struct Touch {
    guarded: bool,
    share_ok: bool,
}

struct PassBCtx<'a> {
    resolved_callees: &'a HashMap<ExprId, Span>,
    fn_index: &'a HashMap<Span, &'a FnDecl>,
    guarded_params: &'a HashMap<Span, HashSet<usize>>,
    type_decls: &'a HashMap<String, &'a TypeDecl>,
    receiver_type: Option<&'a str>,
    mut_params: &'a HashSet<String>,
    param_types: &'a HashMap<String, TypeRef>,
}

fn touch_share_ok(root: &PlaceRoot, ctx: &PassBCtx) -> bool {
    let ty: Option<TypeRef> = match root {
        PlaceRoot::SelfField(f) => ctx.receiver_type.and_then(|tn| ctx.type_decls.get(tn)).and_then(
            |td| field_type_of(td, f),
        ),
        PlaceRoot::SelfBare => ctx.receiver_type.map(|tn| TypeRef::Named {
            path: vec![tn.to_string()],
            generics: vec![],
            span: Span::default(),
        }),
        PlaceRoot::MutParam(p) => ctx.param_types.get(p).cloned(),
    };
    match ty {
        Some(t) => {
            let q = super::CapShareQuery(ctx.type_decls);
            crate::protocols::share_check::is_mut_alias_safe(&q, &t)
        }
        None => false,
    }
}

fn field_type_of(td: &TypeDecl, field: &str) -> Option<TypeRef> {
    let fields: &Vec<RecordField> = match &td.kind {
        TypeDeclKind::Record(fs) => fs,
        _ => return None,
    };
    fields.iter().find(|rf| rf.name == field).map(|rf| rf.ty.clone())
}

/// True iff the resolved target `mut @method`-mutates its receiver.
/// `None` target (unresolved locally — cross-module OR genuinely indirect)
/// is treated conservatively as "yes, assume it mutates" by the caller.
fn callee_mutates(span: Span, ctx: &PassBCtx) -> bool {
    ctx.fn_index
        .get(&span)
        .map(|fd| fd.receiver.as_ref().map(|r| r.mutable).unwrap_or(false))
        .unwrap_or(false)
}

fn record_call_edge(e: &Expr, ctx: &PassBCtx, facts: &mut FnFacts) {
    if let Some(span) = ctx.resolved_callees.get(&e.id).copied() {
        if ctx.fn_index.contains_key(&span) {
            facts.local_callees.insert(span);
        } else {
            facts.cross_module_callee = true;
        }
    }
}

fn analyze_fn_body(fd: &FnDecl, ctx: &PassBCtx) -> FnFacts {
    let mut facts = FnFacts::default();
    let mut guards: Vec<String> = Vec::new();
    match &fd.body {
        FnBody::Block(b) => pb_walk_block(b, ctx, &mut guards, &mut facts),
        FnBody::Expr(e) => pb_walk_expr(e, ctx, &mut guards, &mut facts),
        FnBody::External => {}
    }
    facts
}

fn pb_walk_block(b: &Block, ctx: &PassBCtx, guards: &mut Vec<String>, facts: &mut FnFacts) {
    let mark = guards.len();
    for s in &b.stmts {
        pb_walk_stmt(s, ctx, guards, facts);
    }
    if let Some(t) = &b.trailing {
        pb_walk_expr(t, ctx, guards, facts);
    }
    guards.truncate(mark);
}

fn pb_walk_stmt(s: &Stmt, ctx: &PassBCtx, guards: &mut Vec<String>, facts: &mut FnFacts) {
    match s {
        Stmt::Let(d) => {
            pb_walk_expr(&d.value, ctx, guards, facts);
            if d.consume {
                if let Pattern::Ident { name, .. } = &d.pattern {
                    if matches!(d.value.kind, ExprKind::Call { .. }) {
                        guards.push(name.clone());
                    }
                }
            }
        }
        Stmt::Expr(e) => pb_walk_expr(e, ctx, guards, facts),
        Stmt::Assign { target, value, .. } => {
            pb_walk_expr(value, ctx, guards, facts);
            if let Some(root) = place_root(target, ctx.mut_params) {
                facts.touches.push(Touch {
                    guarded: !guards.is_empty(),
                    share_ok: touch_share_ok(&root, ctx),
                });
            } else {
                pb_walk_expr(target, ctx, guards, facts);
            }
        }
        Stmt::TupleAssign { lhs, rhs, .. } => {
            for e in rhs {
                pb_walk_expr(e, ctx, guards, facts);
            }
            for e in lhs {
                if let Some(root) = place_root(e, ctx.mut_params) {
                    facts.touches.push(Touch {
                        guarded: !guards.is_empty(),
                        share_ok: touch_share_ok(&root, ctx),
                    });
                } else {
                    pb_walk_expr(e, ctx, guards, facts);
                }
            }
        }
        Stmt::Return { value, .. } => {
            if let Some(v) = value {
                pb_walk_expr(v, ctx, guards, facts);
            }
        }
        Stmt::Throw { value, .. } => pb_walk_expr(value, ctx, guards, facts),
        Stmt::Defer { body, .. } => pb_walk_expr(body, ctx, guards, facts),
        Stmt::ConsumeScope { init, body, .. } => {
            pb_walk_expr(init, ctx, guards, facts);
            pb_walk_block(body, ctx, guards, facts);
        }
        Stmt::Const(_)
        | Stmt::Break(_)
        | Stmt::Continue(_)
        | Stmt::AssertStatic { .. }
        | Stmt::Assume { .. }
        | Stmt::Apply { .. }
        | Stmt::Calc { .. }
        | Stmt::Reveal { .. } => {}
    }
}

/// Walk a closure LITERAL's own body with an ISOLATED, fresh guard-stack
/// (`initial`) — NOT the caller's ambient one. A closure is invoked at some
/// later, statically-unknown point (immediately, never, or deep inside a
/// combinator like `with_lock`); the guard state at the point it is
/// TEXTUALLY WRITTEN in the outer body is irrelevant to whether a guard is
/// live when it actually RUNS. `initial` is empty by default, or the
/// single synthetic guard pushed when this closure sits at an inferred
/// [`compute_guarded_params`] position (see `pb_walk_call_args`).
fn pb_walk_closure_body(body_expr: &Expr, ctx: &PassBCtx, initial: Vec<String>, facts: &mut FnFacts) {
    let mut guards = initial;
    match &body_expr.kind {
        ExprKind::Lambda { body, .. } => pb_walk_expr(body, ctx, &mut guards, facts),
        ExprKind::ClosureLight { body, .. } => match body {
            ClosureBody::Expr(e) => pb_walk_expr(e, ctx, &mut guards, facts),
            ClosureBody::Block(b) => pb_walk_block(b, ctx, &mut guards, facts),
        },
        ExprKind::ClosureFull(sb) => pb_walk_fn_sig_body(sb, ctx, &mut guards, facts),
        _ => pb_walk_expr(body_expr, ctx, &mut guards, facts),
    }
}

fn pb_walk_fn_sig_body(sb: &FnSigBody, ctx: &PassBCtx, guards: &mut Vec<String>, facts: &mut FnFacts) {
    match &sb.body {
        FnBody::Block(b) => pb_walk_block(b, ctx, guards, facts),
        FnBody::Expr(e) => pb_walk_expr(e, ctx, guards, facts),
        FnBody::External => {}
    }
}

fn is_closure_literal(e: &Expr) -> bool {
    matches!(
        e.kind,
        ExprKind::Lambda { .. } | ExprKind::ClosureLight { .. } | ExprKind::ClosureFull(_)
    )
}

/// Args + trailing of a `Call` — walked with `guarded_params` applied: an
/// argument at a position the CALLEE (once resolved) is known to invoke
/// under a live lock gets a synthetic guard for its own body; everything
/// else recurses normally (ambient `guards` still applies to non-closure
/// sub-expressions, e.g. a nested call computing an argument value).
fn pb_walk_call_args(
    e: &Expr,
    args: &[CallArg],
    trailing: &Option<Trailing>,
    ctx: &PassBCtx,
    guards: &mut Vec<String>,
    facts: &mut FnFacts,
) {
    let guarded_idx: Option<&HashSet<usize>> = ctx
        .resolved_callees
        .get(&e.id)
        .and_then(|span| ctx.guarded_params.get(span));
    let callee_arity = ctx
        .resolved_callees
        .get(&e.id)
        .and_then(|span| ctx.fn_index.get(span))
        .map(|fd| fd.params.len());

    for (i, a) in args.iter().enumerate() {
        let arg_expr = a.expr();
        if is_closure_literal(arg_expr) && guarded_idx.map(|s| s.contains(&i)).unwrap_or(false) {
            pb_walk_closure_body(arg_expr, ctx, vec!["<synthetic>".to_string()], facts);
        } else {
            pb_walk_expr(arg_expr, ctx, guards, facts);
        }
    }
    if let Some(t) = trailing {
        match t {
            Trailing::Block(b) => pb_walk_block(b, ctx, guards, facts),
            Trailing::LegacyBlockWithParams(tb) => pb_walk_block(&tb.body, ctx, guards, facts),
            Trailing::Fn(sb) => {
                // Trailing closure binds to the LAST positional param —
                // Nova's trailing-closure convention.
                let trailing_idx = callee_arity.map(|n| n.saturating_sub(1));
                let synthetic = trailing_idx
                    .zip(guarded_idx)
                    .map(|(idx, s)| s.contains(&idx))
                    .unwrap_or(false);
                let mut local_guards = if synthetic { vec!["<synthetic>".to_string()] } else { Vec::new() };
                pb_walk_fn_sig_body(sb, ctx, &mut local_guards, facts);
            }
        }
    }
}

fn pb_walk_expr(e: &Expr, ctx: &PassBCtx, guards: &mut Vec<String>, facts: &mut FnFacts) {
    match &e.kind {
        ExprKind::Call { func, args, trailing } => {
            // Touch detection: a method call rooted at `@`/a `mut` param.
            if let ExprKind::Member { obj, .. } = &func.kind {
                if let Some(root) = place_root(obj, ctx.mut_params) {
                    let resolved = ctx.resolved_callees.get(&e.id).copied();
                    let mutates = match resolved {
                        Some(span) if ctx.fn_index.contains_key(&span) => callee_mutates(span, ctx),
                        // Cross-module OR genuinely unresolved (indirect)
                        // target reached FROM a self/mut-param place:
                        // conservative — assume it mutates (§0 default:
                        // "not proven ⇒ unsafe").
                        _ => true,
                    };
                    if mutates {
                        facts.touches.push(Touch {
                            guarded: !guards.is_empty(),
                            share_ok: if guards.is_empty() { touch_share_ok(&root, ctx) } else { false },
                        });
                    }
                }
            }
            record_call_edge(e, ctx, facts);
            pb_walk_expr(func, ctx, guards, facts);
            pb_walk_call_args(e, args, trailing, ctx, guards, facts);
            // Consume-the-guard bookkeeping (mirrors Pass A exactly).
            if let ExprKind::Member { obj, .. } = &func.kind {
                if let ExprKind::Ident(name) = &obj.kind {
                    guards.retain(|g| g != name);
                }
            }
        }
        ExprKind::Lambda { .. } | ExprKind::ClosureLight { .. } | ExprKind::ClosureFull(_) => {
            // A closure literal reached OUTSIDE call-argument position
            // (stored in a `let`, returned, put in a record field, ...):
            // invocation timing is unknown, so it is walked with a FRESH,
            // empty guard-stack — NOT the ambient one (see
            // `pb_walk_closure_body`'s doc). Its touches still count toward
            // THIS enclosing fn's facts (deliberate over-approximation,
            // module doc).
            pb_walk_closure_body(e, ctx, Vec::new(), facts);
        }
        ExprKind::Spawn(inner) => pb_walk_expr(inner, ctx, guards, facts),
        ExprKind::Detach(b) | ExprKind::Blocking(b) => pb_walk_block(b, ctx, guards, facts),
        ExprKind::ParallelFor { iter, body, .. } => {
            pb_walk_expr(iter, ctx, guards, facts);
            pb_walk_block(body, ctx, guards, facts);
        }
        ExprKind::Block(b) => pb_walk_block(b, ctx, guards, facts),
        ExprKind::If { cond, then, else_ } => {
            pb_walk_expr(cond, ctx, guards, facts);
            pb_walk_block(then, ctx, guards, facts);
            match else_ {
                Some(ElseBranch::Block(b)) => pb_walk_block(b, ctx, guards, facts),
                Some(ElseBranch::If(e2)) => pb_walk_expr(e2, ctx, guards, facts),
                None => {}
            }
        }
        ExprKind::IfLet { scrutinee, guard, then, else_, .. } => {
            pb_walk_expr(scrutinee, ctx, guards, facts);
            if let Some(g) = guard {
                pb_walk_expr(g, ctx, guards, facts);
            }
            pb_walk_block(then, ctx, guards, facts);
            match else_ {
                Some(ElseBranch::Block(b)) => pb_walk_block(b, ctx, guards, facts),
                Some(ElseBranch::If(e2)) => pb_walk_expr(e2, ctx, guards, facts),
                None => {}
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            pb_walk_expr(scrutinee, ctx, guards, facts);
            for a in arms {
                if let Some(g) = &a.guard {
                    pb_walk_expr(g, ctx, guards, facts);
                }
                match &a.body {
                    MatchArmBody::Expr(be) => pb_walk_expr(be, ctx, guards, facts),
                    MatchArmBody::Block(bb) => pb_walk_block(bb, ctx, guards, facts),
                }
            }
        }
        ExprKind::For { iter, body, .. } => {
            pb_walk_expr(iter, ctx, guards, facts);
            pb_walk_block(body, ctx, guards, facts);
        }
        ExprKind::While { cond, body, .. } => {
            pb_walk_expr(cond, ctx, guards, facts);
            pb_walk_block(body, ctx, guards, facts);
        }
        ExprKind::WhileLet { scrutinee, guard, body, .. } => {
            pb_walk_expr(scrutinee, ctx, guards, facts);
            if let Some(g) = guard {
                pb_walk_expr(g, ctx, guards, facts);
            }
            pb_walk_block(body, ctx, guards, facts);
        }
        ExprKind::Loop { body, .. } => pb_walk_block(body, ctx, guards, facts),
        ExprKind::Supervised { body, cancel, deadline } => {
            if let Some(c) = cancel {
                pb_walk_expr(c, ctx, guards, facts);
            }
            if let Some(dl) = deadline {
                pb_walk_expr(&dl.expr, ctx, guards, facts);
            }
            pb_walk_block(body, ctx, guards, facts);
        }
        ExprKind::With { bindings, body } => {
            for b in bindings {
                pb_walk_expr(&b.handler, ctx, guards, facts);
            }
            pb_walk_block(body, ctx, guards, facts);
        }
        ExprKind::Forbid { body, .. } | ExprKind::Realtime { body, .. } => {
            pb_walk_block(body, ctx, guards, facts)
        }
        ExprKind::Try(inner) | ExprKind::Bang(inner) | ExprKind::RefArg(inner) => {
            pb_walk_expr(inner, ctx, guards, facts)
        }
        ExprKind::Coalesce(a, b) => {
            pb_walk_expr(a, ctx, guards, facts);
            pb_walk_expr(b, ctx, guards, facts);
        }
        ExprKind::As(inner, _) | ExprKind::Is(inner, _) => pb_walk_expr(inner, ctx, guards, facts),
        ExprKind::Binary { left, right, .. } => {
            pb_walk_expr(left, ctx, guards, facts);
            pb_walk_expr(right, ctx, guards, facts);
        }
        ExprKind::Unary { operand, .. } => pb_walk_expr(operand, ctx, guards, facts),
        ExprKind::Member { obj, .. } => pb_walk_expr(obj, ctx, guards, facts),
        ExprKind::Index { obj, index } => {
            pb_walk_expr(obj, ctx, guards, facts);
            pb_walk_expr(index, ctx, guards, facts);
        }
        ExprKind::TupleLit(elems) => {
            for el in elems {
                pb_walk_expr(el, ctx, guards, facts);
            }
        }
        ExprKind::TurboFish { base, .. } => pb_walk_expr(base, ctx, guards, facts),
        _ => {}
    }
}

// ── Fixed-point graph propagation ───────────────────────────────────────

enum Intra {
    Fixed(FnSafety),
    CandidateSafe(&'static str),
}

fn facts_to_intra(facts: &FnFacts) -> Intra {
    if facts.touches.iter().any(|t| !t.guarded && !t.share_ok) {
        return Intra::Fixed(FnSafety {
            tag: Tag::Unsafe,
            reason: "unguarded_mutation",
            detail: None,
        });
    }
    if facts.cross_module_callee {
        return Intra::Fixed(FnSafety {
            tag: Tag::Undecided,
            reason: "cross_module_callee",
            detail: None,
        });
    }
    if facts.touches.is_empty() {
        Intra::CandidateSafe("no_mutation")
    } else if facts.touches.iter().any(|t| t.guarded) {
        Intra::CandidateSafe("guarded")
    } else {
        Intra::CandidateSafe("share_verified")
    }
}

/// П.1/П.2/П.4: entry point. Computes a [`FnSafety`] for every `Item::Fn`
/// in `module` and, iff `NOVA_DEBUG_FIBER_SAFETY=1`, prints the counters
/// dump this phase's acceptance invariant reads (§4 of the p238-f1 brief:
/// "число бирок == число функций+методов").
pub fn run(module: &Module, resolved_callees: &HashMap<ExprId, Span>) -> HashMap<Span, FnSafety> {
    let fn_index = build_fn_index(module);
    if fn_index.is_empty() {
        return HashMap::new();
    }
    let guarded_params = compute_guarded_params(module);
    let type_decls = build_type_decls(module);

    let mut candidates: HashMap<Span, &'static str> = HashMap::new();
    let mut local_callees_of: HashMap<Span, HashSet<Span>> = HashMap::new();
    let mut final_tag: HashMap<Span, FnSafety> = HashMap::new();

    for (&span, fd) in &fn_index {
        if let Some(reason) = signature_undecided_reason(fd) {
            final_tag.insert(
                span,
                FnSafety { tag: Tag::Undecided, reason, detail: None },
            );
            continue;
        }
        let receiver_type = fd.receiver.as_ref().map(|r| r.type_name.as_str());
        let mut_params: HashSet<String> =
            fd.params.iter().filter(|p| p.is_mut).map(|p| p.name.clone()).collect();
        let param_types: HashMap<String, TypeRef> =
            fd.params.iter().map(|p: &Param| (p.name.clone(), p.ty.clone())).collect();
        let ctx = PassBCtx {
            resolved_callees,
            fn_index: &fn_index,
            guarded_params: &guarded_params,
            type_decls: &type_decls,
            receiver_type,
            mut_params: &mut_params,
            param_types: &param_types,
        };
        let facts = analyze_fn_body(fd, &ctx);
        local_callees_of.insert(span, facts.local_callees.clone());
        match facts_to_intra(&facts) {
            Intra::Fixed(fs) => {
                final_tag.insert(span, fs);
            }
            Intra::CandidateSafe(reason) => {
                candidates.insert(span, reason);
            }
        }
    }

    // Standard iterative dataflow (mirrors `thread_affine_closure`): a
    // candidate settles once every LOCAL callee has settled; Fixed fns
    // settle immediately above and never wait on anything (Unsafe/
    // Undecided cannot become "better" by inspecting callees).
    let mut changed = true;
    while changed {
        changed = false;
        for (&span, &reason) in &candidates {
            if final_tag.contains_key(&span) {
                continue;
            }
            let callees = local_callees_of.get(&span).cloned().unwrap_or_default();
            if callees.iter().all(|c| final_tag.contains_key(c)) {
                let mut worst = Tag::Safe;
                let mut worst_detail: Option<(&'static str, Span)> = None;
                for c in &callees {
                    if let Some(cfs) = final_tag.get(c) {
                        if cfs.tag.rank() > worst.rank() {
                            worst = cfs.tag;
                            let r = if cfs.tag == Tag::Unsafe { "calls_unsafe" } else { "calls_undecided" };
                            worst_detail = Some((r, *c));
                        }
                    }
                }
                let fs = match worst {
                    Tag::Safe => FnSafety { tag: Tag::Safe, reason, detail: None },
                    _ => {
                        let (r, callee_span) = worst_detail.unwrap();
                        let name = fn_index.get(&callee_span).map(|fd| render_fn_name(fd)).unwrap_or_default();
                        FnSafety { tag: worst, reason: r, detail: Some(name) }
                    }
                };
                final_tag.insert(span, fs);
                changed = true;
            }
        }
    }
    // Leftover: candidates stuck in a cycle among themselves (mutual
    // recursion where neither side reaches a settled base case). Resolve
    // using ONLY their own intra-procedural reason — same "cycle with no
    // path to unsafe never gets marked unsafe" reasoning
    // `thread_affine_closure` already relies on.
    for (&span, &reason) in &candidates {
        final_tag.entry(span).or_insert(FnSafety { tag: Tag::Safe, reason, detail: None });
    }

    debug_assert_eq!(
        final_tag.len(),
        fn_index.len(),
        "Plan 238 Ф.1 invariant: every Item::Fn must receive exactly one tag"
    );

    maybe_dump(&final_tag, &fn_index);
    final_tag
}

fn render_fn_name(fd: &FnDecl) -> String {
    match &fd.receiver {
        Some(r) => format!("{}.{}", r.type_name, fd.name),
        None => fd.name.clone(),
    }
}

fn maybe_dump(final_tag: &HashMap<Span, FnSafety>, fn_index: &HashMap<Span, &FnDecl>) {
    if std::env::var("NOVA_DEBUG_FIBER_SAFETY").ok().as_deref() != Some("1") {
        return;
    }
    let total = final_tag.len();
    let mut safe = 0usize;
    let mut unsafe_ = 0usize;
    let mut undecided = 0usize;
    let mut by_reason: HashMap<&'static str, usize> = HashMap::new();
    for fs in final_tag.values() {
        match fs.tag {
            Tag::Safe => safe += 1,
            Tag::Unsafe => unsafe_ += 1,
            Tag::Undecided => undecided += 1,
        }
        *by_reason.entry(fs.reason).or_insert(0) += 1;
    }
    eprintln!("[NOVA_DEBUG_FIBER_SAFETY] Plan 238 Ф.1 dump");
    eprintln!(
        "[NOVA_DEBUG_FIBER_SAFETY] total={} safe={} ({:.1}%) unsafe={} ({:.1}%) undecided={} ({:.1}%)",
        total,
        safe,
        pct(safe, total),
        unsafe_,
        pct(unsafe_, total),
        undecided,
        pct(undecided, total),
    );
    let mut reasons: Vec<(&'static str, usize)> = by_reason.into_iter().collect();
    reasons.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    for (reason, count) in reasons {
        eprintln!(
            "[NOVA_DEBUG_FIBER_SAFETY]   reason={:<22} count={:<6} ({:.1}%)",
            reason,
            count,
            pct(count, total)
        );
    }
    eprintln!(
        "[NOVA_DEBUG_FIBER_SAFETY] invariant tags=={} fns=={}: {}",
        total,
        fn_index.len(),
        total == fn_index.len()
    );

    // Optional, separate opt-in: one line per fn with a globally-stable
    // identity (file_id:start-end — Span's own fields, unique per
    // declaration). `nova check <dir>` re-checks the SAME function once per
    // compile-unit that transitively imports it (each entry file's `Module`
    // flattens its own transitive-import closure), so the aggregate counters
    // above are PER COMPILE-UNIT, not per-program — summing them across a
    // whole `std/src` run double/triple-counts shared functions. This line
    // lets an external script dedupe by identity and get a true
    // whole-corpus aggregate (used for the Ф.1 measurement in
    // `docs/plans/wip/PROGRESS-p238-f1.md`; not part of the brief's own
    // "counters" ask, kept opt-in so it does not bloat the default dump).
    if std::env::var("NOVA_DEBUG_FIBER_SAFETY_VERBOSE").ok().as_deref() == Some("1") {
        // Identity for cross-run dedup: `Span.file_id` is only stable WITHIN
        // one `check_module_impl` call (`SourceMap` — diag.rs — is built
        // fresh per compile-unit, `0` = "this session's own entry file"), so
        // it CANNOT be used to recognise "the same physical declaration"
        // across the many per-entry-file compile-units a whole-directory
        // `nova check <dir>` run performs. `(start, end)` byte offsets ARE
        // stable (computed from the file's own unchanged text) and, paired
        // with the declaration's name + receiver type, are a practically
        // unique key without needing the file path at all.
        for (span, fs) in final_tag {
            let (name, recv) = fn_index
                .get(span)
                .map(|fd| (fd.name.as_str(), fd.receiver.as_ref().map(|r| r.type_name.as_str()).unwrap_or("")))
                .unwrap_or(("?", ""));
            eprintln!(
                "[NOVA_DEBUG_FIBER_SAFETY_FN] fn={}::{}@{}-{} file={} tag={:?} reason={}",
                recv, name, span.start, span.end, span.file_id, fs.tag, fs.reason
            );
        }
    }
}

fn pct(n: usize, total: usize) -> f64 {
    if total == 0 {
        0.0
    } else {
        (n as f64) * 100.0 / (total as f64)
    }
}
