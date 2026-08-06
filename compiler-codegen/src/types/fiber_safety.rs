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
/// graph-propagated reasons (`calls_unsafe`/`calls_undecided`) — read by
/// phase 2's [`render_chain`] together with `detail_span` (the SAME
/// callee's declaration `Span`, i.e. a valid key back into the `tags` map)
/// to walk the cause backward, one hop per `calls_unsafe`/`calls_undecided`
/// link, until a TERMINAL reason (anything else) is reached. `detail_span`
/// is `None` exactly when `detail` is (every non-graph-propagated reason).
#[derive(Clone, Debug)]
pub struct FnSafety {
    pub tag: Tag,
    pub reason: &'static str,
    pub detail: Option<String>,
    pub detail_span: Option<Span>,
}

// ── Pass 0: flat index over Item::Fn (free fns AND methods) ───────────

fn build_fn_index(module: &Module) -> HashMap<Span, &FnDecl> {
    let mut idx = HashMap::new();
    for item in &module.items {
        if let Item::Fn(fd) = item {
            idx.insert(fd.span, fd);
        }
    }
    // Ф.2: mirrors `build_type_decls`'s registry merge below (same file,
    // same established pattern) — registry-only builtin modules
    // (`sync.nv`'s Atomic*/Mutex/RwLock/Semaphore/... among them) are NOT
    // literally `import`ed into `module.items` the way a regular `.nv`
    // file's own declarations are: `external_registry::builtin_sig_
    // modules()` is a SEPARATE resolution channel that `resolved_callees`
    // itself already consults (a call to `AtomicInt.fetch_add` resolves
    // and type-checks fine), but a plain `module.items` walk never sees
    // these declarations. Without this merge, EVERY method on an
    // `Atomic*`/`Mutex`/`CancelToken`/... type is invisible to `fn_index`
    // — measured live gap: `AtomicInt.fetch_add` inside a `spawn` body
    // reported "belongs to a DIFFERENT compile unit" even checking
    // `std/src` itself, where nothing is actually cross-package. Registry
    // entries are keyed by their OWN stable declaration `Span` (the
    // registry's content is loaded once, not re-parsed per compile unit),
    // so this merge is safe to repeat on every `run()`/`check_seed_
    // points()` call — `or_insert` leaves a real, module-declared `fn` of
    // the same span untouched (can't happen in practice, spans are
    // per-file-unique, but matches `build_type_decls`'s own precedence
    // convention for consistency).
    for ext_mod in crate::codegen::external_registry::builtin_sig_modules() {
        for item in &ext_mod.items {
            if let Item::Fn(fd) = item {
                idx.entry(fd.span).or_insert(fd);
            }
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

/// Companion to [`place_root`] for a FREE-function call's ARGUMENTS (not a
/// receiver) — see `pb_walk_expr`'s `ExprKind::Ident(_)` callee branch.
/// `true` = this argument's syntactic root is provably NOT `@`/a `mut`
/// param of the CURRENT fn (a plain non-`mut` param, a local, or a value
/// with no traceable `Ident` root at all — a literal/computed temporary,
/// which cannot alias anything named). Walks through `Member`/zero-arg
/// method `Call` chains (`b.ptr()`) and the same wrapper set `place_root`
/// already strips. Deliberately conservative on anything it cannot trace
/// through (a `Call` with a non-`Member` callee, e.g. a further nested
/// free-fn call) — `false`, same "can't prove ⇒ not exempted" default.
fn arg_root_safe(e: &Expr, mut_params: &HashSet<String>) -> bool {
    match &e.kind {
        ExprKind::SelfAccess => false,
        ExprKind::Ident(name) => !mut_params.contains(name),
        ExprKind::Member { obj, .. } => arg_root_safe(obj, mut_params),
        ExprKind::Index { obj, .. } => arg_root_safe(obj, mut_params),
        ExprKind::As(inner, _) => arg_root_safe(inner, mut_params),
        ExprKind::TurboFish { base, .. }
        | ExprKind::Try(base)
        | ExprKind::Bang(base)
        | ExprKind::RefArg(base) => arg_root_safe(base, mut_params),
        // `b.ptr()`-style zero-arg method call — trace through to the
        // receiver; a call with a non-`Member` callee (nested free-fn
        // call) is NOT traced (conservative — no known live pattern needs
        // it, and guessing wrong here would be a false EXEMPTION, the
        // wrong direction to be wrong in).
        ExprKind::Call { func, .. } => match &func.kind {
            ExprKind::Member { obj, .. } => arg_root_safe(obj, mut_params),
            _ => false,
        },
        // No traceable `Ident` root at all (literal, arithmetic, etc.) —
        // cannot alias anything named.
        ExprKind::IntLit(_)
        | ExprKind::FloatLit(_)
        | ExprKind::StrLit(_)
        | ExprKind::BoolLit(_)
        | ExprKind::UnitLit
        | ExprKind::CharLit(_)
        | ExprKind::NullPtrLit
        | ExprKind::Binary { .. }
        | ExprKind::Unary { .. } => true,
        _ => false,
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
            if crate::protocols::share_check::is_mut_alias_safe(&q, &t) {
                return true;
            }
            // Ф.2 companion fix (D446 sync brick 2 — linearity, not brick
            // 3 — synchronization): a touch on a `consume`/LINEAR-typed
            // receiver (`TcpStream`/`Transaction`/... — D415's own named
            // examples) is safe for a DIFFERENT reason than `#share`: a
            // linear type's single-ownership discipline (D131/consume)
            // ALREADY guarantees no second alias exists to race with —
            // that is what makes `spawn consume c { ... }` sound in the
            // FIRST place (D415 §4). `is_mut_alias_safe` alone only
            // implements brick 3 (audited synchronization); without this,
            // EVERY `mut`-parameter of a linear type (the ordinary way a
            // moved-in resource is threaded through a call chain once
            // inside its owning fiber) was blanket Unsafe("unguarded_
            // mutation") — measured live gap: `TcpStream.read`/`.write`
            // inside `examples/flagship/aggregator`'s `spawn`/`detach`
            // bodies (36 seed-point rejections, virtually all bottoming
            // out in `TcpStream.*`/`TlsStream.*` touching their OWN
            // receiver — a receiver that, by construction, was `consume`d
            // into this fiber and can have no second owner).
            typeref_named_base(&t)
                .and_then(|b| ctx.type_decls.get(b))
                .map(|td| td.consume)
                .unwrap_or(false)
        }
        None => false,
    }
}

/// Strips `ro`/`mut`/`uninit` binding-modifier wrappers to the named base
/// type — mirrors `TypeCheckCtx::typeref_named_base` (types/mod.rs), not
/// shared directly (this module has no `&self` `TypeCheckCtx` to call it
/// on — same small-helper-duplication precedent as `callee_bare_name`).
fn typeref_named_base(ty: &TypeRef) -> Option<&str> {
    match ty {
        TypeRef::Named { path, .. } => path.last().map(|s| s.as_str()),
        TypeRef::Readonly(inner, _) | TypeRef::Mut(inner, _) | TypeRef::Uninit(inner, _) => {
            typeref_named_base(inner)
        }
        _ => None,
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
            let mut skip_edge = false;
            if let ExprKind::Member { obj, name } = &func.kind {
                if let Some(root) = place_root(obj, ctx.mut_params) {
                    let resolved = ctx.resolved_callees.get(&e.id).copied();
                    let mutates = match resolved {
                        Some(span) if ctx.fn_index.contains_key(&span) => callee_mutates(span, ctx),
                        // Приёмка интегратора 2026-08-06 — measured live gap:
                        // raw-pointer intrinsic methods (`*T`'s hardcoded
                        // dispatch set, `types/mod.rs`'s own `is_raw_pointer_
                        // intrinsic_method` — "read"/"write"/"offset"/"dist"/
                        // ...) are NEVER `Item::Fn` (special-cased directly in
                        // codegen), so `resolved_callees` has NO entry for
                        // them — this arm's conservative "unresolved from a
                        // self/mut-param place ⇒ assume mutates" default fired
                        // on `@ptr.offset(i)` inside `str.find`/`.rfind`
                        // (`std/src/runtime/string/search.nv`), poisoning BOTH
                        // (and transitively EVERY caller — `str.find` is used
                        // pervasively) even though `.offset(..)` is PURE
                        // pointer arithmetic — it computes a new address, it
                        // never touches memory at all, let alone writes
                        // through `@`. `offset`/`dist` are the only two
                        // entries in that fixed set with this property (the
                        // rest — `read`/`write`/`copy_from`/... — genuinely
                        // access memory through the pointer and stay
                        // conservative).
                        _ if name == "offset" || name == "dist" => false,
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
                    // Приёмка интегратора 2026-08-06 — measured live gap:
                    // `BucketTable.bucket_for` (`nova-polaris/src/middleware/
                    // ratelimit.nv`) does `consume g = @mutex.lock(); ...;
                    // @buckets.insert(..)` — the DIRECT touch above is
                    // correctly marked `guarded: true` (not itself flagged),
                    // but `record_call_edge` (below, unconditional) STILL
                    // added `HashMap.insert` as a graph edge regardless of
                    // guard state — and `HashMap.insert`'s OWN tag is
                    // Unsafe("unguarded_mutation") UNCONDITIONALLY (it
                    // touches ITS OWN `@`, `HashMap` is ordinary, not
                    // `#share`/`consume`) — so `bucket_for` inherited
                    // `calls_unsafe` from ITS OWN correctly-guarded callee,
                    // silently OVERRIDING the guard verdict the direct-touch
                    // check just computed. The guard protects exactly this:
                    // a call reached while `g` is live, on the SAME `@`-
                    // rooted receiver, whose ENTIRE unsafety is its own
                    // direct self-touch (not something deeper/unrelated) —
                    // skip the edge so the fixed-point does not re-derive
                    // "unguarded" from a call that plainly IS guarded here.
                    if !guards.is_empty() && mutates {
                        if let Some(span) = resolved {
                            if ctx.fn_index.contains_key(&span) {
                                skip_edge = true;
                            }
                        }
                    }
                } else {
                    // Ф.2 fix (measured live gap — `Path.join_path`'s `out.push
                    // (..)`, `read_dir`'s local `Vec`-building, pervasive across
                    // `std`/flagship): `obj` resolves to NEITHER `@` nor a
                    // `mut`-param of THIS fn — a purely LOCAL, non-escaping
                    // receiver (the SAME "local mut accumulator is not a touch"
                    // exemption the module doc already states for a DIRECT
                    // assignment `out = ...`; `record_call_edge`, unlike
                    // `place_root`'s OWN touch check above, did not previously
                    // apply that exemption to a METHOD CALL on such a receiver
                    // — `out.push(x)` unconditionally added `Vec.push` as a
                    // graph edge regardless of `out`'s locality, so ANY caller
                    // of an ordinary (non-`#share`) collection's mutating method
                    // permanently inherited that method's OWN unavoidable
                    // "touches its own `@`" tag — transitively poisoning nearly
                    // every real caller (measured: flagship `examples/flagship/
                    // aggregator` alone produced 37 seed-point rejections before
                    // this fix, virtually all bottoming out in `Vec.*`/`String.*`
                    // mutating their OWN receiver, not in anything actually
                    // reachable from OUTSIDE the local call). If the resolved
                    // target is a LOCALLY-known (`ctx.fn_index`) `mut @method`,
                    // skip the graph edge: whatever it does to ITS OWN receiver
                    // is exactly as safe as this fn mutating `out` directly,
                    // which Pass B already exempts. A genuinely opaque target
                    // (unresolved, or resolved outside this module) still
                    // records the edge — this fix narrows ONLY the provably-
                    // safe "local receiver, known same-module mutator" case,
                    // it does not touch the `@`/`mut`-param path above at all
                    // (a receiver ACTUALLY rooted at `@`/a param is unaffected
                    // — that is the genuinely observable-outside-this-call
                    // case D446 exists to catch, e.g. the metrics.nv/
                    // ratelimit.nv shape).
                    if let Some(span) = ctx.resolved_callees.get(&e.id).copied() {
                        if ctx.fn_index.contains_key(&span) && callee_mutates(span, ctx) {
                            skip_edge = true;
                        }
                    }
                }
            } else if matches!(func.kind, ExprKind::Ident(_)) {
                // Приёмка интегратора 2026-08-06 — companion fix, free-
                // function twin of the Member-receiver case above. Measured
                // live gap: `net_addr_loopback_into(port as u16, b.ptr())`/
                // `fs_stat_size(img.ptr())`-style raw-pointer FFI wrappers
                // (`std/src/net/ffi.nv`, `std/src/fs/ffi.nv`) — their OWN
                // signature is unconditionally `no_body`+"takes a raw
                // pointer" (nothing can vouch for an opaque `*T`/`*mut T`
                // structurally), but EVERY pointer-bearing argument at
                // THIS call site traces back to a value `b`/`img` this fn
                // owns exclusively (a fresh local, or its own non-`mut`
                // parameter — never `@`/a `mut` param, checked by
                // `arg_root_safe` below, same "not observable outside this
                // call" reasoning as `place_root`'s touch check). Checked
                // against the target's OWN SIGNATURE directly (`signature_
                // undecided_reason`, not its settled tag — the fixed-point
                // hasn't run yet at this point in `run`, ordering over
                // `fn_index` is not guaranteed) — `no_body` specifically,
                // since that is the ONLY signature-level reason this
                // exemption is meant to cover (a `generic`/`extern`/
                // `fn_param_in_sig` target is left untouched, still records
                // the edge, conservative default unchanged).
                if let Some(span) = ctx.resolved_callees.get(&e.id).copied() {
                    if let Some(target_fd) = ctx.fn_index.get(&span) {
                        if signature_undecided_reason(target_fd) == Some("no_body")
                            && args.iter().all(|a| arg_root_safe(a.expr(), ctx.mut_params))
                        {
                            skip_edge = true;
                        }
                    }
                }
            }
            if !skip_edge {
                record_call_edge(e, ctx, facts);
            }
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
            detail_span: None,
        });
    }
    if facts.cross_module_callee {
        return Intra::Fixed(FnSafety {
            tag: Tag::Undecided,
            reason: "cross_module_callee",
            detail: None,
            detail_span: None,
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
        if let Some(reason) = signature_undecided_reason(fd) {
            // Ф.2 (D446 sync brick 3): an extern (no-body) METHOD on a
            // `#share`-verified receiver type is that type's OWN audited
            // synchronized primitive (Atomic*/Mutex/CancelToken/... — the
            // white list's actual FFI implementation) — not "unprovable".
            // Measured live gap: `CancelToken.cancel`/`.is_cancelled`
            // (both `export extern "nova" fn`, `CancelToken` is `#share`,
            // D415-audited) were blanket Undecided("no_body") before this
            // fix — the white list existed on the TYPE but was never
            // consulted for its own extern methods. `is_mut_alias_safe`
            // (D415's structural walk, VERBATIM reuse — same call
            // `touch_share_ok` below already makes) on the receiver type
            // itself decides.
            if reason == "no_body" {
                // Ф.2: only an INSTANCE receiver (`@`) has an actual `self`
                // value whose OWN `#share`-ness is the right question —
                // route it through the receiver-type check below. A
                // STATIC-receiver extern (`SocketAddr.loopback(port)`,
                // `Receiver::kind == Static`, no `@` in its body at all —
                // essentially a namespaced free fn) has NOTHING to check
                // `#share` on; the args-safety check (the SAME one
                // receiverless free fns use) is the right question for it
                // instead. Measured live gap: `SocketAddr.loopback`
                // (`export extern "nova" fn SocketAddr.loopback(port u16)
                // -> Self`) was blanket Undecided("no_body") — the
                // receiver-type check tried (wrongly) to verify `#share`
                // on `SocketAddr` itself, which is not what a static ctor
                // even touches.
                let is_instance_recv = fd
                    .receiver
                    .as_ref()
                    .map(|r| r.kind == crate::ast::ReceiverKind::Instance)
                    .unwrap_or(false);
                if is_instance_recv {
                    if let Some(tn) = receiver_type {
                        let recv_ty = TypeRef::Named {
                            path: vec![tn.to_string()],
                            generics: vec![],
                            span: Span::default(),
                        };
                        let q = super::CapShareQuery(&type_decls);
                        if crate::protocols::share_check::is_mut_alias_safe(&q, &recv_ty) {
                            final_tag.insert(
                                span,
                                FnSafety {
                                    tag: Tag::Safe,
                                    reason: "share_verified_extern",
                                    detail: None,
                                    detail_span: None,
                                },
                            );
                            continue;
                        }
                    }
                } else {
                    // Ф.2 companion fix — a receiverless-or-STATIC extern
                    // (no `@` value to check `#share` on — either no
                    // receiver at all, or a static/namespaced one) whose
                    // ENTIRE signature (every param, the return type) is
                    // `mut`-alias-safe (D415's own "ro/primitive ⇒
                    // shareable" brick 1, `is_mut_alias_safe` again —
                    // reused, not a new rule) receives nothing it could
                    // hand a caller-visible mutable alias through, so
                    // whatever the FFI body does internally cannot touch
                    // caller-observable shared state via ITS ARGUMENTS. A
                    // bare `mut T` parameter type is EXCLUDED on purpose
                    // (`TypeRef::Mut` fails `is_mut_alias_safe`'s own check
                    // already — no special-casing needed) since that DOES
                    // let the FFI write through a caller-visible place.
                    // Measured live gaps: `assert(cond bool)`/`assert(cond
                    // bool, msg str)` (`std/src/prelude/runtime.nv`, used
                    // pervasively inside `spawn` bodies across the ENTIRE
                    // test corpus) and `SocketAddr.loopback(port u16)` —
                    // both were blanket Undecided("no_body") despite being
                    // unable to reach any state beyond their own by-value
                    // args.
                    // NOTE: `is_mut_alias_safe` (`Access::Mut`, fixed) is
                    // the WRONG predicate here — it is D415's "is aliasing
                    // this MUT BINDING safe" check, which refuses EVERY
                    // bare scalar unconditionally (`bool`/`str`/`int` —
                    // "a writable scalar cell aliased across fibers IS the
                    // race", `share_check.rs`'s own comment) REGARDLESS of
                    // whether this specific parameter is actually mutable —
                    // it assumes `Access::Mut` universally, so `assert(cond
                    // bool)` failed THIS check even though `cond` is an
                    // ordinary by-VALUE bool copy, not an alias to
                    // anything. `is_alias_read_safe` (`Access::Read`) is
                    // the right predicate for a plain by-value PARAMETER —
                    // the callee reads/copies it, it does not hand back a
                    // caller-visible mutable place. A parameter EXPLICITLY
                    // typed `mut T` is excluded by hand (not through
                    // `share_rec`'s `TypeRef::Mut` arm, which just forwards
                    // whatever `access` it's given — see that arm's own
                    // comment: the mut/ro distinction there is meant to
                    // come from the BINDING at a capture site, not from a
                    // signature's own type annotation) — a genuine `mut`
                    // parameter DOES let the FFI write through a
                    // caller-visible place and must not be blessed here.
                    let q = super::CapShareQuery(&type_decls);
                    let param_safe = |ty: &TypeRef| -> bool {
                        if matches!(ty, TypeRef::Mut(..)) {
                            return false;
                        }
                        // `never` (a fn that does not return normally —
                        // `panic(msg str) -> never`) produces NO value
                        // ever, so it cannot hand back a caller-visible
                        // alias — trivially safe, but `share_rec` has no
                        // dedicated arm for it (parses as an ordinary
                        // `TypeRef::Named{path:["never"]}`, not in
                        // `NOVA_PRIMITIVES`, falls through to the unknown-
                        // type refusal). `Self` on a STATIC receiver
                        // (`SocketAddr.loopback(..) -> Self`) resolves to
                        // a BRAND NEW instance of the receiver type being
                        // constructed and returned — not an alias to any
                        // EXISTING caller-visible state — also trivially
                        // safe regardless of what `is_alias_read_safe`
                        // would say about the receiver type's OWN general
                        // `#share`-ness (a fresh value has no second owner
                        // yet). Both measured live gaps on `std/src/net`.
                        if let TypeRef::Named { path, generics, .. } = ty {
                            if generics.is_empty() && path.len() == 1 {
                                if path[0] == "never" || path[0] == "Self" {
                                    return true;
                                }
                            }
                        }
                        // Приёмка интегратора 2026-08-06: a raw pointer
                        // to a NON-`mut` pointee (`*T` — the parser's OWN
                        // canonical default, `parser/mod.rs`: "`*T` →
                        // Pointer(T) (default ≡ ro target)"; only `*mut T`
                        // → `Pointer(Mut(T))` is writable) cannot be
                        // written through AT ALL — `share_check.rs`'s
                        // `share_rec` refuses EVERY `TypeRef::Pointer`
                        // unconditionally (it does not consult the pointee
                        // modifier), which is the right call FOR ITS OWN
                        // callers (D415 capture-check — a captured `mut`
                        // BINDING's aliasing is a different question from a
                        // parameter's declared TYPE) but too coarse here.
                        // Measured live gap: `net_addr_port(addr *u8) ->
                        // u16`/`fs_stat_size(img *u8) -> int` (both plain
                        // `*T`, read-only by the type itself) were blanket
                        // Undecided("no_body"). Handled LOCALLY (not by
                        // editing `share_check.rs`, which other, unrelated
                        // callers rely on staying exactly as strict as it
                        // is) — only a bare `TypeRef::Pointer` whose
                        // pointee is NOT itself `TypeRef::Mut` is treated
                        // as safe; `*mut T` still falls through to
                        // `is_alias_read_safe`'s unconditional refusal.
                        if let TypeRef::Pointer(inner, _) = ty {
                            if !matches!(inner.as_ref(), TypeRef::Mut(..)) {
                                return true;
                            }
                        }
                        crate::protocols::share_check::is_alias_read_safe(&q, ty)
                    };
                    let all_safe = fd.params.iter().all(|p| param_safe(&p.ty))
                        && fd.return_type.as_ref().map_or(true, |rt| param_safe(rt));
                    if all_safe {
                        final_tag.insert(
                            span,
                            FnSafety {
                                tag: Tag::Safe,
                                reason: "share_verified_extern_args",
                                detail: None,
                                detail_span: None,
                            },
                        );
                        continue;
                    }
                }
            }
            // Ф.2 narrow refinement: a GENERIC fn whose body makes ZERO
            // calls and ZERO touches (`T` is purely inert data — cast/
            // copied, never itself the target of a dispatched operation)
            // cannot do anything unsafe regardless of what `T` is
            // instantiated to — measured live gap: `fn[T Ints] T
            // @to_millis() -> Duration => { nanos: (@ as i64)*1_000_000 }`
            // (`std/src/time/duration/core.nv`) was blanket
            // Undecided("generic") despite being a pure arithmetic leaf,
            // and this shape is pervasive in the real corpus (`Duration`'s
            // whole `to_*` conversion family). Deliberately NARROW — a
            // generic fn that calls ANYTHING (even something itself
            // `Safe`) stays Undecided: bound-sensitive call safety is NOT
            // implemented here (D446 Ф.1 report §6 pt.4 — explicitly
            // deferred, "work of the first wave after measurement"); this
            // only stops over-rejecting the total-leaf case.
            if reason == "generic" {
                let facts = analyze_fn_body(fd, &ctx);
                if facts.touches.is_empty() && facts.local_callees.is_empty() && !facts.cross_module_callee {
                    final_tag.insert(
                        span,
                        FnSafety {
                            tag: Tag::Safe,
                            reason: "generic_leaf_no_op",
                            detail: None,
                            detail_span: None,
                        },
                    );
                    continue;
                }
            }
            final_tag.insert(
                span,
                FnSafety { tag: Tag::Undecided, reason, detail: None, detail_span: None },
            );
            continue;
        }
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
                    Tag::Safe => FnSafety { tag: Tag::Safe, reason, detail: None, detail_span: None },
                    _ => {
                        let (r, callee_span) = worst_detail.unwrap();
                        let name = fn_index.get(&callee_span).map(|fd| render_fn_name(fd)).unwrap_or_default();
                        FnSafety { tag: worst, reason: r, detail: Some(name), detail_span: Some(callee_span) }
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
        final_tag.entry(span).or_insert(FnSafety { tag: Tag::Safe, reason, detail: None, detail_span: None });
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

// ============================================================================
// Plan 238 Ф.2 (D446 "Ф.8-НОВАЯ", план раздел "П.2"/"Критерий приёмки"):
// enforcement at fiber-SEEDING points. Phase 1 built the total bирка (tag)
// over the resolved call graph and deliberately enforced NOTHING (measurement
// only). This section is the missing other half: `spawn`/`detach`/`parallel
// for` bodies are exactly the places D446 §1 names as where a SECOND fiber
// gets added — `main` (and every ordinary fn body outside one of these three
// constructs) is the base unit and never seeds on its own. `supervised { }`
// is NOT a fourth boundary of its own (D446 Ф.6 decision 1's "надзор,
// порождающий детей" cashes out as: a supervised body spawns children via an
// ordinary `spawn` INSIDE it — that `spawn` node is what actually seeds, and
// `find_seeds_*` below reaches it exactly like any other, since it recurses
// through `Supervised`'s body).
// ============================================================================

/// One `Call` reached SYNCHRONOUSLY inside a seed body — its own span (for
/// diagnostic placement), `ExprId` (the `resolved_callees` lookup key), and
/// the BARE callee name (last segment only — `foo`/`Type.method`'s `method`/
/// `a::b::c`'s `c`) used by the two fallbacks below when `resolved_callees`
/// itself has no entry (see their doc comments for WHY that gap exists and
/// is not itself a soundness hole).
struct SeedCall {
    id: ExprId,
    site: Span,
    name: Option<String>,
    /// `(type_name, method_name)` for the generic-static-receiver call
    /// shapes (`Vec[T].new(...)`/`[]u8.new(...)`) — see [`static_receiver_
    /// type_method`]'s doc for why bare-`name` fallback alone cannot
    /// disambiguate these (measured: `[]u8.new()`'s bare name "new" collides
    /// with EVERY OTHER type's constructor in `name_index`).
    static_key: Option<(String, String)>,
    /// True iff this is a `obj.method(..)` call whose `obj` is a bare
    /// `Ident` bound by a `Let` WITHIN this seed body (never captured from
    /// the enclosing scope) — see the push site's doc comment.
    local_receiver: bool,
}

/// Mirrors `types/mod.rs`'s `generic_static_receiver` (duplicated — same
/// established "small, stable snippet, cheaper to copy than to plumb a
/// cross-module dependency" precedent as `callee_bare_name`/`call_callee_
/// name` below): recognises the TWO generic-static-receiver shapes the
/// parser produces — `Vec[T].new(...)` (`Member{ obj: TurboFish{ base:
/// Ident|Path(1) }, name }`) and the `[]T` slice-sugar spelling
/// (`Member{ obj: Path(["__array", elem]), name }`, normalized to the
/// `"Vec"` key — `callnorm.rs`'s own `static_key` convention). `None` for
/// every other call shape (this is intentionally narrow — ordinary
/// instance/free calls already resolve via `resolved_callees` directly or
/// via the bare-name fallback; this ONLY exists for the one measured shape
/// neither of those two handles).
fn static_receiver_type_method(func: &Expr) -> Option<(String, String)> {
    let ExprKind::Member { obj, name } = &func.kind else { return None };
    match &obj.kind {
        ExprKind::TurboFish { base, .. } => match &base.kind {
            ExprKind::Ident(n) => Some((n.clone(), name.clone())),
            ExprKind::Path(parts) if parts.len() == 1 => Some((parts[0].clone(), name.clone())),
            _ => None,
        },
        ExprKind::Path(parts) if parts.len() == 2 && parts[0] == "__array" => {
            Some(("Vec".to_string(), name.clone()))
        }
        _ => None,
    }
}

/// Mirrors `types/mod.rs`'s own `call_callee_name` (duplicated, not shared —
/// same "small, stable snippet, cheaper to copy than to plumb a shared
/// dependency across module boundaries" precedent that function's own doc
/// comment already establishes for an identical need).
fn callee_bare_name(func: &Expr) -> Option<String> {
    match &func.kind {
        ExprKind::Ident(n) => Some(n.clone()),
        ExprKind::Member { name, .. } => Some(name.clone()),
        ExprKind::Path(segs) => segs.last().cloned(),
        _ => None,
    }
}

/// D446 sync-side brick 3 ("белый список синхронизированных... концы
/// каналов"): channel send/recv and the timer-channel constructors are
/// COMPILER BUILTINS with no corresponding `Item::Fn` anywhere in `std`
/// (verified by grep — `std/src/concurrency/timer.nv`'s own comment on
/// `ChanReader.close_after` says it outright: "via compiler builtin, not
/// via this fn"; channel `send`/`recv`/`try_send`/`try_recv` have no
/// declaration in `std/src/concurrency/*.nv`/`prelude/*.nv` at all — the
/// channel TYPE itself is compiler-intrinsic, not a `std`-defined generic
/// type with ordinary methods). A call to one of these can therefore NEVER
/// have a `resolved_callees` entry — that is not "target statically
/// unknown" (D446 §4's actual concern, a value of function type invoked
/// through a param/field), it is a plumbing fact about where the dispatch
/// lives. Channel endpoints are explicitly the D446 white list's OWN
/// example of a safe-by-construction synchronized primitive — treating a
/// call to one as `Safe` here is applying that brick, not suppressing a
/// finding. A narrow, explicitly-named allowlist (mirrors the established
/// `is_suspend_op_path`/`is_raw_pointer_intrinsic_method` precedent
/// elsewhere in this file's sibling module for the identical class of
/// problem — a compiler-builtin op with no AST `FnDecl` to hang a tag on).
fn is_builtin_channel_or_timer_op(name: &str) -> bool {
    matches!(name, "send" | "recv" | "try_send" | "try_recv" | "close_after" | "tick_every")
}

/// Ф.2 entry point: walk EVERY `Item::Fn`/`Item::Test` body in `module`
/// (`module.items` is already flat — folder-modules included, `Module.
/// items`'s own doc: "остаются flat for backward compat") looking for a
/// `spawn`/`detach`/`parallel for` node anywhere in it (not just top-level —
/// `find_seeds_expr`/`find_seeds_block` recurse through the WHOLE body).
/// `tags` is the SAME map [`run`] returned for this module (Phase 1's
/// bирка) — reused, not recomputed. `fn_index` is rebuilt locally (cheap,
/// module-sized single pass — mirrors [`run`]'s own, kept separate so this
/// fn's signature does not depend on `run`'s internals).
pub fn check_seed_points(
    module: &Module,
    resolved_callees: &HashMap<ExprId, Span>,
    tags: &HashMap<Span, FnSafety>,
    errors: &mut Vec<crate::diag::Diagnostic>,
) {
    let fn_index = build_fn_index(module);
    // Name-based fallback index (bare fn/method NAME → every declaration
    // span sharing it, receiver ignored) — see `check_one_seed`'s doc for
    // WHY this is needed (a `resolved_callees` coverage gap for certain
    // call shapes, measured on the real `std` corpus: generic-blanket
    // primitive-receiver methods like `T@to_millis()`/`T@sleep()` and the
    // `__array::Elem.new(...)` slice-sugar constructor shape never get a
    // `resolved_callees` entry even though they ARE ordinary, unambiguous
    // `Item::Fn`s). Only consulted when a call's target is ambiguous by
    // NAME (2+ spans) does the fallback decline to guess (same "ambiguity
    // → don't guess" stance `resolve_tuple_call_return` already documents
    // elsewhere in this crate for an analogous problem).
    let mut name_index: HashMap<String, Vec<Span>> = HashMap::new();
    // `(receiver type, method)` fallback for the generic-static-receiver
    // shapes `static_receiver_type_method` recognises — see [`SeedCall`]'s
    // doc: `[]u8.new()`'s bare name "new" collides with every OTHER type's
    // constructor in `name_index`, this index disambiguates by receiver too.
    let mut type_method_index: HashMap<(String, String), Vec<Span>> = HashMap::new();
    for (&span, fd) in &fn_index {
        name_index.entry(fd.name.clone()).or_default().push(span);
        if let Some(r) = &fd.receiver {
            type_method_index
                .entry((r.type_name.clone(), fd.name.clone()))
                .or_default()
                .push(span);
        }
    }
    for item in &module.items {
        match item {
            Item::Fn(fd) => match &fd.body {
                FnBody::Block(b) => find_seeds_block(
                    b, resolved_callees, tags, &fn_index, &name_index, &type_method_index, errors,
                ),
                FnBody::Expr(e) => find_seeds_expr(
                    e, resolved_callees, tags, &fn_index, &name_index, &type_method_index, errors,
                ),
                FnBody::External => {}
            },
            Item::Test(td) => find_seeds_block(
                &td.body, resolved_callees, tags, &fn_index, &name_index, &type_method_index, errors,
            ),
            _ => {}
        }
    }
}

/// Full recursive walk (same `ExprKind` coverage as Pass B's `pb_walk_expr`)
/// looking for a seed node ANYWHERE in the tree — including nested inside
/// call arguments, closures, branches, etc. At each `Spawn`/`Detach`/
/// `ParallelFor`, checks that ONE seed's own call surface (`check_one_seed_*`)
/// and THEN keeps recursing into its body too, so a seed nested inside
/// another seed gets its own, independent check (matches `own_fiber_call_
/// names_expr`'s established "stop the SURFACE scan at a nested boundary,
/// but the OUTER walk still visits it" split, reused here as two functions
/// instead of one flag).
fn find_seeds_block(
    b: &Block,
    rc: &HashMap<ExprId, Span>,
    tags: &HashMap<Span, FnSafety>,
    fn_index: &HashMap<Span, &FnDecl>,
    name_index: &HashMap<String, Vec<Span>>,
    type_method_index: &HashMap<(String, String), Vec<Span>>,
    errors: &mut Vec<crate::diag::Diagnostic>,
) {
    for s in &b.stmts {
        find_seeds_stmt(s, rc, tags, fn_index, name_index, type_method_index, errors);
    }
    if let Some(t) = &b.trailing {
        find_seeds_expr(t, rc, tags, fn_index, name_index, type_method_index, errors);
    }
}

fn find_seeds_stmt(
    s: &Stmt,
    rc: &HashMap<ExprId, Span>,
    tags: &HashMap<Span, FnSafety>,
    fn_index: &HashMap<Span, &FnDecl>,
    name_index: &HashMap<String, Vec<Span>>,
    type_method_index: &HashMap<(String, String), Vec<Span>>,
    errors: &mut Vec<crate::diag::Diagnostic>,
) {
    match s {
        Stmt::Let(d) => find_seeds_expr(&d.value, rc, tags, fn_index, name_index, type_method_index, errors),
        Stmt::Expr(e) => find_seeds_expr(e, rc, tags, fn_index, name_index, type_method_index, errors),
        Stmt::Assign { target, value, .. } => {
            find_seeds_expr(target, rc, tags, fn_index, name_index, type_method_index, errors);
            find_seeds_expr(value, rc, tags, fn_index, name_index, type_method_index, errors);
        }
        Stmt::TupleAssign { lhs, rhs, .. } => {
            for e in lhs {
                find_seeds_expr(e, rc, tags, fn_index, name_index, type_method_index, errors);
            }
            for e in rhs {
                find_seeds_expr(e, rc, tags, fn_index, name_index, type_method_index, errors);
            }
        }
        Stmt::Return { value, .. } => {
            if let Some(v) = value {
                find_seeds_expr(v, rc, tags, fn_index, name_index, type_method_index, errors);
            }
        }
        Stmt::Throw { value, .. } => find_seeds_expr(value, rc, tags, fn_index, name_index, type_method_index, errors),
        Stmt::Defer { body, .. } => find_seeds_expr(body, rc, tags, fn_index, name_index, type_method_index, errors),
        Stmt::ConsumeScope { init, body, .. } => {
            find_seeds_expr(init, rc, tags, fn_index, name_index, type_method_index, errors);
            find_seeds_block(body, rc, tags, fn_index, name_index, type_method_index, errors);
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

fn find_seeds_expr(
    e: &Expr,
    rc: &HashMap<ExprId, Span>,
    tags: &HashMap<Span, FnSafety>,
    fn_index: &HashMap<Span, &FnDecl>,
    name_index: &HashMap<String, Vec<Span>>,
    type_method_index: &HashMap<(String, String), Vec<Span>>,
    errors: &mut Vec<crate::diag::Diagnostic>,
) {
    match &e.kind {
        ExprKind::Spawn(inner) => {
            let mut calls = Vec::new();
            let mut locals = HashSet::new();
            collect_seed_calls_expr(inner, &mut calls, &mut locals);
            check_one_seed("spawn", &calls, rc, tags, fn_index, name_index, type_method_index, errors);
            find_seeds_expr(inner, rc, tags, fn_index, name_index, type_method_index, errors);
        }
        ExprKind::Detach(body) => {
            let mut calls = Vec::new();
            let mut locals = HashSet::new();
            collect_seed_calls_block(body, &mut calls, &mut locals);
            check_one_seed("detach", &calls, rc, tags, fn_index, name_index, type_method_index, errors);
            find_seeds_block(body, rc, tags, fn_index, name_index, type_method_index, errors);
        }
        ExprKind::ParallelFor { iter, body, .. } => {
            find_seeds_expr(iter, rc, tags, fn_index, name_index, type_method_index, errors);
            let mut calls = Vec::new();
            let mut locals = HashSet::new();
            collect_seed_calls_block(body, &mut calls, &mut locals);
            check_one_seed("parallel for", &calls, rc, tags, fn_index, name_index, type_method_index, errors);
            find_seeds_block(body, rc, tags, fn_index, name_index, type_method_index, errors);
        }
        ExprKind::Blocking(b) => find_seeds_block(b, rc, tags, fn_index, name_index, type_method_index, errors),
        ExprKind::Call { func, args, trailing } => {
            find_seeds_expr(func, rc, tags, fn_index, name_index, type_method_index, errors);
            for a in args {
                find_seeds_expr(a.expr(), rc, tags, fn_index, name_index, type_method_index, errors);
            }
            if let Some(t) = trailing {
                match t {
                    Trailing::Block(b) => find_seeds_block(b, rc, tags, fn_index, name_index, type_method_index, errors),
                    Trailing::LegacyBlockWithParams(tb) => {
                        find_seeds_block(&tb.body, rc, tags, fn_index, name_index, type_method_index, errors)
                    }
                    Trailing::Fn(sb) => match &sb.body {
                        FnBody::Block(b) => find_seeds_block(b, rc, tags, fn_index, name_index, type_method_index, errors),
                        FnBody::Expr(ex) => find_seeds_expr(ex, rc, tags, fn_index, name_index, type_method_index, errors),
                        FnBody::External => {}
                    },
                }
            }
        }
        ExprKind::Lambda { body, .. } => find_seeds_expr(body, rc, tags, fn_index, name_index, type_method_index, errors),
        ExprKind::ClosureLight { body, .. } => match body {
            ClosureBody::Expr(ex) => find_seeds_expr(ex, rc, tags, fn_index, name_index, type_method_index, errors),
            ClosureBody::Block(b) => find_seeds_block(b, rc, tags, fn_index, name_index, type_method_index, errors),
        },
        ExprKind::ClosureFull(sb) => match &sb.body {
            FnBody::Block(b) => find_seeds_block(b, rc, tags, fn_index, name_index, type_method_index, errors),
            FnBody::Expr(ex) => find_seeds_expr(ex, rc, tags, fn_index, name_index, type_method_index, errors),
            FnBody::External => {}
        },
        ExprKind::Block(b) => find_seeds_block(b, rc, tags, fn_index, name_index, type_method_index, errors),
        ExprKind::If { cond, then, else_ } => {
            find_seeds_expr(cond, rc, tags, fn_index, name_index, type_method_index, errors);
            find_seeds_block(then, rc, tags, fn_index, name_index, type_method_index, errors);
            match else_ {
                Some(ElseBranch::Block(b)) => find_seeds_block(b, rc, tags, fn_index, name_index, type_method_index, errors),
                Some(ElseBranch::If(e2)) => find_seeds_expr(e2, rc, tags, fn_index, name_index, type_method_index, errors),
                None => {}
            }
        }
        ExprKind::IfLet { scrutinee, guard, then, else_, .. } => {
            find_seeds_expr(scrutinee, rc, tags, fn_index, name_index, type_method_index, errors);
            if let Some(g) = guard {
                find_seeds_expr(g, rc, tags, fn_index, name_index, type_method_index, errors);
            }
            find_seeds_block(then, rc, tags, fn_index, name_index, type_method_index, errors);
            match else_ {
                Some(ElseBranch::Block(b)) => find_seeds_block(b, rc, tags, fn_index, name_index, type_method_index, errors),
                Some(ElseBranch::If(e2)) => find_seeds_expr(e2, rc, tags, fn_index, name_index, type_method_index, errors),
                None => {}
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            find_seeds_expr(scrutinee, rc, tags, fn_index, name_index, type_method_index, errors);
            for a in arms {
                if let Some(g) = &a.guard {
                    find_seeds_expr(g, rc, tags, fn_index, name_index, type_method_index, errors);
                }
                match &a.body {
                    MatchArmBody::Expr(be) => find_seeds_expr(be, rc, tags, fn_index, name_index, type_method_index, errors),
                    MatchArmBody::Block(bb) => find_seeds_block(bb, rc, tags, fn_index, name_index, type_method_index, errors),
                }
            }
        }
        ExprKind::For { iter, body, .. } => {
            find_seeds_expr(iter, rc, tags, fn_index, name_index, type_method_index, errors);
            find_seeds_block(body, rc, tags, fn_index, name_index, type_method_index, errors);
        }
        ExprKind::While { cond, body, .. } => {
            find_seeds_expr(cond, rc, tags, fn_index, name_index, type_method_index, errors);
            find_seeds_block(body, rc, tags, fn_index, name_index, type_method_index, errors);
        }
        ExprKind::WhileLet { scrutinee, guard, body, .. } => {
            find_seeds_expr(scrutinee, rc, tags, fn_index, name_index, type_method_index, errors);
            if let Some(g) = guard {
                find_seeds_expr(g, rc, tags, fn_index, name_index, type_method_index, errors);
            }
            find_seeds_block(body, rc, tags, fn_index, name_index, type_method_index, errors);
        }
        ExprKind::Loop { body, .. } => find_seeds_block(body, rc, tags, fn_index, name_index, type_method_index, errors),
        ExprKind::Supervised { body, cancel, deadline } => {
            if let Some(c) = cancel {
                find_seeds_expr(c, rc, tags, fn_index, name_index, type_method_index, errors);
            }
            if let Some(dl) = deadline {
                find_seeds_expr(&dl.expr, rc, tags, fn_index, name_index, type_method_index, errors);
            }
            find_seeds_block(body, rc, tags, fn_index, name_index, type_method_index, errors);
        }
        ExprKind::With { bindings, body } => {
            for b in bindings {
                find_seeds_expr(&b.handler, rc, tags, fn_index, name_index, type_method_index, errors);
            }
            find_seeds_block(body, rc, tags, fn_index, name_index, type_method_index, errors);
        }
        ExprKind::Forbid { body, .. } | ExprKind::Realtime { body, .. } => {
            find_seeds_block(body, rc, tags, fn_index, name_index, type_method_index, errors)
        }
        ExprKind::Try(inner) | ExprKind::Bang(inner) | ExprKind::RefArg(inner) => {
            find_seeds_expr(inner, rc, tags, fn_index, name_index, type_method_index, errors)
        }
        ExprKind::Coalesce(a, b) => {
            find_seeds_expr(a, rc, tags, fn_index, name_index, type_method_index, errors);
            find_seeds_expr(b, rc, tags, fn_index, name_index, type_method_index, errors);
        }
        ExprKind::As(inner, _) | ExprKind::Is(inner, _) => {
            find_seeds_expr(inner, rc, tags, fn_index, name_index, type_method_index, errors)
        }
        ExprKind::Binary { left, right, .. } => {
            find_seeds_expr(left, rc, tags, fn_index, name_index, type_method_index, errors);
            find_seeds_expr(right, rc, tags, fn_index, name_index, type_method_index, errors);
        }
        ExprKind::Unary { operand, .. } => find_seeds_expr(operand, rc, tags, fn_index, name_index, type_method_index, errors),
        ExprKind::Member { obj, .. } => find_seeds_expr(obj, rc, tags, fn_index, name_index, type_method_index, errors),
        ExprKind::Index { obj, index } => {
            find_seeds_expr(obj, rc, tags, fn_index, name_index, type_method_index, errors);
            find_seeds_expr(index, rc, tags, fn_index, name_index, type_method_index, errors);
        }
        ExprKind::TupleLit(elems) => {
            for el in elems {
                find_seeds_expr(el, rc, tags, fn_index, name_index, type_method_index, errors);
            }
        }
        ExprKind::TurboFish { base, .. } => find_seeds_expr(base, rc, tags, fn_index, name_index, type_method_index, errors),
        _ => {}
    }
}

/// The "stop at a NESTED boundary" half — collects every `Call` reached
/// SYNCHRONOUSLY in `e`'s execution as part of THIS seed body (recursing
/// through ordinary control flow/closures-defined-and-inspected-here), but
/// does NOT descend into a nested `Spawn`/`Detach`/`ParallelFor`/`Blocking` —
/// that one is a SEPARATE seed, checked independently by `find_seeds_expr`
/// (mirrors `own_fiber_call_names_expr`'s identical stance, D441/A-V10).
fn collect_seed_calls_expr(e: &Expr, out: &mut Vec<SeedCall>, locals: &mut HashSet<String>) {
    match &e.kind {
        ExprKind::Call { func, args, trailing } => {
            // Ф.2 (приёмка 2026-08-06 — измеренный live gap, `Vec.resize`/
            // `.push`/... blanket-Unsafe from touching THEIR OWN receiver,
            // reached via `obj.method(..)` where `obj` is a name bound by a
            // `Let` WITHIN this very seed body — never captured from the
            // enclosing scope at all, provably fresh/non-escaping. Mirrors
            // `record_call_edge`'s OWN "local receiver, mutating method"
            // exemption (Pass B, used when computing ANOTHER function's own
            // facts) — this is the SAME reasoning applied at the SEED point
            // itself, which `record_call_edge`'s fix could not reach (a
            // seed body is not an `Item::Fn`, its own calls are gated
            // directly against the callee's GLOBAL tag, which is Unsafe
            // unconditionally for an ordinary `mut @method`). See
            // `check_resolved_target`'s use of this flag for the exact,
            // narrow condition under which it is honored (self-touch only,
            // never a deeper `calls_unsafe/undecided` propagation).
            let local_receiver = match &func.kind {
                ExprKind::Member { obj, .. } => match &obj.kind {
                    ExprKind::Ident(name) => locals.contains(name),
                    _ => false,
                },
                _ => false,
            };
            out.push(SeedCall {
                id: e.id,
                site: e.span,
                name: callee_bare_name(func),
                static_key: static_receiver_type_method(func),
                local_receiver,
            });
            collect_seed_calls_expr(func, out, locals);
            for a in args {
                collect_seed_calls_expr(a.expr(), out, locals);
            }
            if let Some(t) = trailing {
                match t {
                    Trailing::Block(b) => collect_seed_calls_block(b, out, locals),
                    Trailing::LegacyBlockWithParams(tb) => collect_seed_calls_block(&tb.body, out, locals),
                    Trailing::Fn(sb) => match &sb.body {
                        FnBody::Block(b) => collect_seed_calls_block(b, out, locals),
                        FnBody::Expr(ex) => collect_seed_calls_expr(ex, out, locals),
                        FnBody::External => {}
                    },
                }
            }
        }
        ExprKind::Lambda { body, .. } => collect_seed_calls_expr(body, out, locals),
        ExprKind::ClosureLight { body, .. } => match body {
            ClosureBody::Expr(ex) => collect_seed_calls_expr(ex, out, locals),
            ClosureBody::Block(b) => collect_seed_calls_block(b, out, locals),
        },
        ExprKind::ClosureFull(sb) => match &sb.body {
            FnBody::Block(b) => collect_seed_calls_block(b, out, locals),
            FnBody::Expr(ex) => collect_seed_calls_expr(ex, out, locals),
            FnBody::External => {}
        },
        // Nested seed boundary — its own, independent check (`find_seeds_*`
        // reaches it separately); do not fold its calls into THIS seed's
        // surface.
        ExprKind::Spawn(_)
        | ExprKind::Detach(_)
        | ExprKind::ParallelFor { .. }
        | ExprKind::Blocking(_) => {}
        ExprKind::Block(b) => collect_seed_calls_block(b, out, locals),
        ExprKind::If { cond, then, else_ } => {
            collect_seed_calls_expr(cond, out, locals);
            collect_seed_calls_block(then, out, locals);
            match else_ {
                Some(ElseBranch::Block(b)) => collect_seed_calls_block(b, out, locals),
                Some(ElseBranch::If(e2)) => collect_seed_calls_expr(e2, out, locals),
                None => {}
            }
        }
        ExprKind::IfLet { scrutinee, guard, then, else_, .. } => {
            collect_seed_calls_expr(scrutinee, out, locals);
            if let Some(g) = guard {
                collect_seed_calls_expr(g, out, locals);
            }
            collect_seed_calls_block(then, out, locals);
            match else_ {
                Some(ElseBranch::Block(b)) => collect_seed_calls_block(b, out, locals),
                Some(ElseBranch::If(e2)) => collect_seed_calls_expr(e2, out, locals),
                None => {}
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            collect_seed_calls_expr(scrutinee, out, locals);
            for a in arms {
                if let Some(g) = &a.guard {
                    collect_seed_calls_expr(g, out, locals);
                }
                match &a.body {
                    MatchArmBody::Expr(be) => collect_seed_calls_expr(be, out, locals),
                    MatchArmBody::Block(bb) => collect_seed_calls_block(bb, out, locals),
                }
            }
        }
        ExprKind::For { iter, body, .. } => {
            collect_seed_calls_expr(iter, out, locals);
            collect_seed_calls_block(body, out, locals);
        }
        ExprKind::While { cond, body, .. } => {
            collect_seed_calls_expr(cond, out, locals);
            collect_seed_calls_block(body, out, locals);
        }
        ExprKind::WhileLet { scrutinee, guard, body, .. } => {
            collect_seed_calls_expr(scrutinee, out, locals);
            if let Some(g) = guard {
                collect_seed_calls_expr(g, out, locals);
            }
            collect_seed_calls_block(body, out, locals);
        }
        ExprKind::Loop { body, .. } => collect_seed_calls_block(body, out, locals),
        ExprKind::Supervised { body, cancel, deadline } => {
            if let Some(c) = cancel {
                collect_seed_calls_expr(c, out, locals);
            }
            if let Some(dl) = deadline {
                collect_seed_calls_expr(&dl.expr, out, locals);
            }
            collect_seed_calls_block(body, out, locals);
        }
        ExprKind::With { bindings, body } => {
            for b in bindings {
                collect_seed_calls_expr(&b.handler, out, locals);
            }
            collect_seed_calls_block(body, out, locals);
        }
        ExprKind::Forbid { body, .. } | ExprKind::Realtime { body, .. } => {
            collect_seed_calls_block(body, out, locals)
        }
        ExprKind::Try(inner) | ExprKind::Bang(inner) | ExprKind::RefArg(inner) => {
            collect_seed_calls_expr(inner, out, locals)
        }
        ExprKind::Coalesce(a, b) => {
            collect_seed_calls_expr(a, out, locals);
            collect_seed_calls_expr(b, out, locals);
        }
        ExprKind::As(inner, _) | ExprKind::Is(inner, _) => collect_seed_calls_expr(inner, out, locals),
        ExprKind::Binary { left, right, .. } => {
            collect_seed_calls_expr(left, out, locals);
            collect_seed_calls_expr(right, out, locals);
        }
        ExprKind::Unary { operand, .. } => collect_seed_calls_expr(operand, out, locals),
        ExprKind::Member { obj, .. } => collect_seed_calls_expr(obj, out, locals),
        ExprKind::Index { obj, index } => {
            collect_seed_calls_expr(obj, out, locals);
            collect_seed_calls_expr(index, out, locals);
        }
        ExprKind::TupleLit(elems) => {
            for el in elems {
                collect_seed_calls_expr(el, out, locals);
            }
        }
        ExprKind::TurboFish { base, .. } => collect_seed_calls_expr(base, out, locals),
        _ => {}
    }
}

fn collect_seed_calls_block(b: &Block, out: &mut Vec<SeedCall>, locals: &mut HashSet<String>) {
    for s in &b.stmts {
        collect_seed_calls_stmt(s, out, locals);
    }
    if let Some(t) = &b.trailing {
        collect_seed_calls_expr(t, out, locals);
    }
}

/// Collects every `Ident`-bound name reachable in `p` into `locals` — used
/// to recognise "this receiver was bound by a `Let` WITHIN this seed body"
/// (see `collect_seed_calls_expr`'s `ExprKind::Call` arm doc). Partial
/// coverage (`Ident`/`Tuple`/`Binding` — the common `let` shapes) is safe
/// to under-approximate: a MISSED binding just means the "local receiver"
/// exemption does not fire for it, staying MORE conservative, never less.
fn collect_pattern_names(p: &Pattern, locals: &mut HashSet<String>) {
    match p {
        Pattern::Ident { name, .. } => {
            locals.insert(name.clone());
        }
        Pattern::Tuple(elems, _) => {
            for e in elems {
                collect_pattern_names(e, locals);
            }
        }
        Pattern::Binding { name, inner, .. } => {
            locals.insert(name.clone());
            collect_pattern_names(inner, locals);
        }
        _ => {}
    }
}

fn collect_seed_calls_stmt(s: &Stmt, out: &mut Vec<SeedCall>, locals: &mut HashSet<String>) {
    match s {
        Stmt::Let(d) => {
            collect_seed_calls_expr(&d.value, out, locals);
            collect_pattern_names(&d.pattern, locals);
        }
        Stmt::Expr(e) => collect_seed_calls_expr(e, out, locals),
        Stmt::Assign { target, value, .. } => {
            collect_seed_calls_expr(target, out, locals);
            collect_seed_calls_expr(value, out, locals);
        }
        Stmt::TupleAssign { lhs, rhs, .. } => {
            for e in lhs {
                collect_seed_calls_expr(e, out, locals);
            }
            for e in rhs {
                collect_seed_calls_expr(e, out, locals);
            }
        }
        Stmt::Return { value, .. } => {
            if let Some(v) = value {
                collect_seed_calls_expr(v, out, locals);
            }
        }
        Stmt::Throw { value, .. } => collect_seed_calls_expr(value, out, locals),
        Stmt::Defer { body, .. } => collect_seed_calls_expr(body, out, locals),
        Stmt::ConsumeScope { init, body, .. } => {
            collect_seed_calls_expr(init, out, locals);
            collect_seed_calls_block(body, out, locals);
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

/// Приёмка интегратора 2026-08-06 (p238-f2): `E_FIBER_INDIRECT_CALL`'s
/// on/off switch. Default OFF (unset/anything other than `"1"`) — indirect
/// calls (D446 §4) need a type-level safety marker on the function-typed
/// parameter to be migratable, and that marker's syntax is an OPEN owner
/// decision (not yet made) — enforcing unconditionally today breaks corpus
/// patterns (`race2[T]`, task queues, route dispatch) with NO available
/// fix. Fixtures exercising the indirect path run with `NOVA_FIBER_
/// INDIRECT=1` set explicitly (see each fixture's own header comment).
fn indirect_enforcement_enabled() -> bool {
    std::env::var("NOVA_FIBER_INDIRECT").ok().as_deref() == Some("1")
}

/// П.2 (Ф.8-НОВАЯ): for every call reached in ONE seed body, resolve its
/// target and gate it.
///
/// `resolved_callees` has NO entry for two measured, DIFFERENT reasons —
/// conflating them was this phase's first false-positive class (measured on
/// `nova check std/src`: 124 "no entry" calls, of which only 2 were a
/// genuine indirect closure call — the rest were plumbing gaps, not D446
/// §4's actual concern):
///  1. a compiler-builtin op with no `Item::Fn` at all (channel `send`/
///     `recv`/timer constructors — [`is_builtin_channel_or_timer_op`]);
///  2. an ordinary, unambiguous `Item::Fn` that this checker version simply
///     never wrote a `resolved_callees` entry for (measured: generic-
///     blanket primitive-receiver methods like `T@to_millis()`, and the
///     `__array::Elem.new(...)` slice-sugar constructor shape) — resolved
///     via `name_index`'s unique-name fallback.
/// Only once BOTH fallbacks decline (no builtin match, and the name is
/// either unknown or ambiguous in `name_index`) is a call treated as
/// GENUINELY indirect (D446 §4 — a value of function type invoked through a
/// param/field, target set undecidable) and flagged `E_FIBER_INDIRECT_
/// CALL` — "target undecidable ⇒ unsafe", false rejections expected/
/// accepted per П.2.
///
/// A call whose target DOES resolve (directly, or via the name fallback) is
/// gated by [`check_resolved_target`]: `Safe` passes silently; anything
/// else is `E_FIBER_UNSAFE_CALL` with the backward chain
/// ([`render_chain`]) to the root cause; a target with NO tag at all is a
/// genuine cross-compile-unit gap (Ф.1 report §6 point 3) — same
/// conservative default, no new channel needed.
fn check_one_seed(
    boundary: &'static str,
    calls: &[SeedCall],
    rc: &HashMap<ExprId, Span>,
    tags: &HashMap<Span, FnSafety>,
    fn_index: &HashMap<Span, &FnDecl>,
    name_index: &HashMap<String, Vec<Span>>,
    type_method_index: &HashMap<(String, String), Vec<Span>>,
    errors: &mut Vec<crate::diag::Diagnostic>,
) {
    for call in calls {
        let direct = rc.get(&call.id).copied();
        // Fallback 1 — `(type, method)` generic-static-receiver shape
        // (`[]u8.new()`/`Vec[T].new()`): if EVERY candidate overload the
        // receiver+name key resolves to shares the SAME tag, the identity
        // ambiguity (which overload) doesn't matter — the SAFETY verdict is
        // unanimous, so resolve to the first candidate. If they disagree,
        // decline (stay conservative — matches the plain bare-name
        // fallback's "ambiguous → don't guess" stance, just one step less
        // strict since here "ambiguous" only bites when the VERDICT, not
        // merely the identity, is actually in question).
        let via_static_key = call.static_key.as_ref().and_then(|key| {
            let candidates = type_method_index.get(key)?;
            let first = *candidates.first()?;
            let first_tag = tags.get(&first).map(|fs| fs.tag);
            if candidates.iter().all(|c| tags.get(c).map(|fs| fs.tag) == first_tag) {
                Some(first)
            } else {
                None
            }
        });
        let resolved = direct.or(via_static_key).or_else(|| {
            let name = call.name.as_deref()?;
            if is_builtin_channel_or_timer_op(name) {
                return None; // handled below — whitelisted, not a fallback target.
            }
            match name_index.get(name)?.as_slice() {
                [only] => Some(*only),
                _ => None, // zero or ambiguous — decline, stay conservative.
            }
        });
        match resolved {
            Some(target) => check_resolved_target(
                boundary, call.site, target, call.local_receiver, tags, fn_index, errors,
            ),
            None => {
                let whitelisted = call.name.as_deref().map(is_builtin_channel_or_timer_op).unwrap_or(false);
                if whitelisted {
                    continue; // D446 sync brick 3 — channel endpoint, safe by construction.
                }
                // Приёмка интегратора 2026-08-06 (p238-f2): E_FIBER_UNSAFE_
                // CALL (direct — a RESOLVED target whose бирка isn't Safe)
                // stays enforced unconditionally — direct enforcement is
                // sound and the corpus is migratable (see PROGRESS-p238-f2
                // §"background.nv"/№364-style fixes). E_FIBER_INDIRECT_CALL
                // (a call whose target CANNOT be resolved at all — D446 §4)
                // is a DIFFERENT question: it needs a type-level safety
                // marker on the function-typed parameter (owner-designed,
                // Plan 248-adjacent, not yet specified) to be migratable
                // rather than just suppressed — enforcing it unconditionally
                // TODAY breaks `std`/`nova-polaris` on patterns (`race2[T]`,
                // background task queues, route dispatch) that have no fix
                // available yet. Gated behind `NOVA_FIBER_INDIRECT=1`
                // (default OFF) — mechanism stays fully wired (still walks
                // every seed, still resolves via all three fallbacks/
                // whitelist first), only the FINAL diagnostic emission for
                // a genuinely-undecidable target is suppressed by default.
                if !indirect_enforcement_enabled() {
                    continue;
                }
                errors.push(crate::diag::Diagnostic::new(
                    format!(
                        "[E_FIBER_INDIRECT_CALL] indirect call inside a `{boundary}` \
                         body — the callee is reached through a parameter/field of \
                         function type, so its concrete target is not statically known \
                         (D446 §4/Ф.8-НОВАЯ П.2: an indeterminable target set is treated \
                         as unsafe — false rejections are expected and accepted here, a \
                         silent pass is the failure mode this wave closes). Under M:N \
                         scheduling this fiber may run concurrently with, or migrate \
                         across threads from, its parent/siblings — an unproven callee \
                         reached this way could touch unguarded shared state. Fix: call \
                         the concrete function BEFORE `{boundary}` (capture its result \
                         `ro`), or change the parameter/field's type to a closed set your \
                         call site can resolve statically.",
                        boundary = boundary,
                    ),
                    call.site,
                ));
            }
        }
    }
}

fn check_resolved_target(
    boundary: &'static str,
    site: Span,
    target: Span,
    local_receiver: bool,
    tags: &HashMap<Span, FnSafety>,
    fn_index: &HashMap<Span, &FnDecl>,
    errors: &mut Vec<crate::diag::Diagnostic>,
) {
    match tags.get(&target) {
        // Приёмка интегратора 2026-08-06: `local_receiver` — this call's
        // receiver is a name bound by a `Let` WITHIN this seed body (never
        // captured). If the ENTIRE reason `target` isn't `Safe` is its own
        // DIRECT self-touch (`unguarded_mutation` — reached its own `@`/
        // mut-param, not a further `calls_unsafe`/`calls_undecided` hop
        // into something else), that self-touch is happening on a
        // receiver PROVABLY local to this fiber — exactly as safe as this
        // seed body mutating it directly would be (Pass B's OWN "local mut
        // accumulator is not a touch" rule, `record_call_edge`'s twin fix,
        // applied here at the seed point where that fix could not reach).
        // Deliberately narrow: does NOT fire when the unsafety comes from
        // something DEEPER (`calls_unsafe`/`calls_undecided`) — that risk
        // exists regardless of receiver locality. Measured live gap:
        // `Vec.resize`/`.push`/... on a freshly-`let`-bound local inside a
        // `spawn` body (`std/src/net/byte_surface_test.nv`) was blanket
        // `E_FIBER_UNSAFE_CALL` even though nothing here is shared at all.
        Some(fs) if local_receiver && fs.reason == "unguarded_mutation" => {}
        None => {
            let name = fn_index.get(&target).map(|fd| render_fn_name(fd)).unwrap_or_default();
            errors.push(crate::diag::Diagnostic::new(
                format!(
                    "[E_FIBER_UNSAFE_CALL] call to `{name}` inside a `{boundary}` \
                     body — `{name}` belongs to a DIFFERENT compile unit (its M:N-\
                     safety tag was computed in a separate `run`, not this one — \
                     D446 Ф.1 report §6 point 3's cross-compile-unit gap) and is \
                     therefore UNPROVEN here. D446 Ф.8-НОВАЯ П.2: not proven ⇒ \
                     unsafe, by default. Fix: if `{name}` is genuinely M:N-safe, \
                     recompile the crossing package alongside this one so the tag \
                     is visible, or restructure so the call happens BEFORE \
                     `{boundary}`.",
                    name = name, boundary = boundary,
                ),
                site,
            ));
        }
        Some(fs) if fs.tag != Tag::Safe => {
            let name = fn_index.get(&target).map(|fd| render_fn_name(fd)).unwrap_or_default();
            let chain = render_chain(target, tags, fn_index);
            errors.push(crate::diag::Diagnostic::new(
                format!(
                    "[E_FIBER_UNSAFE_CALL] call to `{name}` inside a `{boundary}` \
                     body — its M:N-safety tag is `{tag:?}`, not `Safe` (D446 Ф.8-\
                     НОВАЯ П.2: not proven ⇒ unsafe, by default — every fn/method \
                     gets a tag, an unrecognised shape is a REJECTION, not a silent \
                     pass). Cause: {reason}. Chain: {chain}. Under M:N scheduling \
                     this fiber may run concurrently with, or migrate across \
                     threads from, its parent/siblings — an unguarded mutation \
                     reached through this call is a data race. Fix: guard the \
                     touched state under a live lock (`consume g = x.lock()`), \
                     make its type `#share`-safe and structurally verified, or \
                     restructure so the call happens BEFORE `{boundary}` (its \
                     result captured `ro` into the fiber).",
                    name = name,
                    boundary = boundary,
                    tag = fs.tag,
                    reason = terminal_reason_text(chain_terminal_reason(target, tags)),
                    chain = chain,
                ),
                site,
            ));
        }
        Some(_) => {} // Safe — no diagnostic.
    }
}

/// Ш.7 (D446/plan Ф.8-НОВАЯ, "диагностика — обход назад"): follow `detail_
/// span` through `calls_unsafe`/`calls_undecided` links, one hop per link,
/// rendering `` `name` → `name` → … `` until a TERMINAL reason (anything
/// else — `unguarded_mutation`/`no_body`/`generic`/`extern`/`fn_param_in_
/// sig`/`cross_module_callee`) is reached. Capped at 32 hops as a defensive
/// guard against a malformed chain — the fixed-point construction in
/// [`run`] is acyclic along `calls_unsafe`/`calls_undecided` edges by
/// design (a `Fixed` fn's tag never changes once set, so a cycle purely
/// among candidates resolves to `Safe`+its own intra reason, never a
/// `calls_*` reason — see `run`'s own "Leftover" comment), so 32 is slack,
/// not a real ceiling.
fn render_chain(start: Span, tags: &HashMap<Span, FnSafety>, fn_index: &HashMap<Span, &FnDecl>) -> String {
    let mut hops: Vec<String> = Vec::new();
    let mut span = start;
    for _ in 0..32 {
        let name = fn_index.get(&span).map(|fd| render_fn_name(fd)).unwrap_or_else(|| "?".to_string());
        hops.push(format!("`{}`", name));
        match tags.get(&span) {
            Some(fs) if matches!(fs.reason, "calls_unsafe" | "calls_undecided") => match fs.detail_span {
                Some(next) => span = next,
                None => break,
            },
            _ => break,
        }
    }
    hops.join(" → ")
}

/// The reason at the END of `render_chain`'s walk (the terminal cause) —
/// walked separately (not threaded back out of `render_chain` as a return
/// value) to keep `render_chain`'s own signature focused on rendering.
fn chain_terminal_reason(start: Span, tags: &HashMap<Span, FnSafety>) -> &'static str {
    let mut span = start;
    for _ in 0..32 {
        match tags.get(&span) {
            Some(fs) if matches!(fs.reason, "calls_unsafe" | "calls_undecided") => match fs.detail_span {
                Some(next) => span = next,
                None => return fs.reason,
            },
            Some(fs) => return fs.reason,
            None => return "unknown",
        }
    }
    "unknown"
}

fn terminal_reason_text(reason: &str) -> &'static str {
    match reason {
        "unguarded_mutation" => "mutates reachable state with no live lock and no `#share`-verified type",
        "no_body" => "bottoms out in an external (no-body) fn — nothing to prove from",
        "generic" => "bottoms out in a generic fn with no safety bound on its type parameter",
        "extern" => "bottoms out in an `extern`-declared fn",
        "fn_param_in_sig" => "bottoms out in a fn taking a function-typed parameter — its target is not statically closed",
        "cross_module_callee" => "bottoms out in a call whose target is outside this compile unit",
        _ => "reason not recovered (defensive fallback — should not normally occur)",
    }
}
