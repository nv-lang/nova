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
///
/// Plan 238 Ф.3 (D446 §4/§5 амендмент, 2026-08-06): `fn_param_in_sig` —
/// "any fn taking a function-typed parameter is blanket Undecided,
/// regardless of what it does with it" — is RETRACTED here. Ф.1/Ф.2 needed
/// it because there was no way to tell "this fn invokes its own closure
/// parameter somewhere a fiber boundary could reach" from "this fn merely
/// ACCEPTS a closure and never calls it dangerously" (`Mutex.with_lock`
/// itself never touches unguarded state — only the CALLER-supplied closure
/// might, and that closure runs UNDER `with_lock`'s own guard in the first
/// place, per the ALREADY-EXISTING `compute_guarded_params`/Pass-A
/// mechanism). Ф.3 replaces the blanket rule with a PRECISE one:
/// [`compute_required_params`] infers, per function-typed PARAMETER
/// (not per function), whether THAT parameter is ever invoked from inside
/// a `spawn`/`detach`/`parallel for` reached in the declaring fn's own
/// body — directly, or transitively by forwarding the parameter, unchanged,
/// into another local fn's own required parameter (the graph fixed point
/// mirrors Ф.1's own tag propagation, "same idea, new axis"). The
/// ENCLOSING function's own tag no longer degrades just because it HAS a
/// function-typed parameter — it settles from its OWN touches/callees
/// exactly like any other function (measured effect: `Mutex.with_lock`/
/// `RwLock.with_read`/`with_write`/`Semaphore.with_permit` — none of which
/// touch `@`/a `mut` param themselves — settle `Tag::Safe` directly instead
/// of being blanket-Undecided, `PROGRESS-p238-f3.md` §1). The actual risk
/// (an UNSAFE closure reaching a required parameter) is checked
/// SEPARATELY, at every call site that PASSES an argument into a required
/// parameter position (`check_param_passing`, `E_FIBER_UNSAFE_ARG`) — where
/// the argument's own captures are syntactically visible, unlike here.
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
        ExprKind::Supervised { body, cancel, deadline, on_timeout } => {
            if let Some(c) = cancel {
                gp_walk_expr(c, params, guards, found);
            }
            if let Some(dl) = deadline {
                gp_walk_expr(&dl.expr, params, guards, found);
            }
            if let Some(oh) = on_timeout {
                gp_walk_expr(oh, params, guards, found);
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
    /// Plan 238 Ф.3 (D446 §4/§5 амендмент) — measured live gap: at least
    /// one call reached through a self/mut-param-rooted receiver's OWN
    /// DECLARED FIELD whose type is itself function-typed
    /// (`StreamBody { priv f fn() -> Option[[]u8] }`'s `(@f)()`), left
    /// UNRESOLVED by `resolved_callees` (there is no `Item::Fn` named `f`
    /// to resolve to — the field IS the callable value). Ф.1/Ф.2's Call-arm
    /// touch detection cannot syntactically tell "a field access being
    /// invoked" from "an unresolved METHOD call" — both are `Member{obj,
    /// name}` — and previously defaulted BOTH to the conservative
    /// `unguarded_mutation` (proven-Unsafe) touch class. That default is
    /// right for a genuinely opaque unresolved method, but WRONG for THIS
    /// syntactic shape: a call through a function-typed field is exactly
    /// D446 §4's "непрямые вызовы" territory (indeterminable target, not a
    /// proven state mutation) — the SAME confidence class this section's
    /// own `unguarded_mut_param`/`fn_param_in_sig`-successor reasons
    /// already occupy, not the harder-evidenced `@`-touch class. See
    /// `field_call_is_func_type`'s call site in `pb_walk_expr` (the ONLY
    /// producer of this flag) for the exact structural check.
    field_call_unresolved: bool,
}

struct Touch {
    guarded: bool,
    share_ok: bool,
    /// Приёмка интегратора 2026-08-06 (третий раунд) — distinguishes the
    /// TWO shapes `place_root` can return, which are NOT the same danger:
    /// `true` (`SelfBare`/`SelfField`) — the receiver's OWN persistent
    /// state (the `MetricsRegistry`/`BucketTable` shape D446 exists to
    /// catch: an object with a LIFETIME beyond one call, reachable from
    /// many call sites/fibers — an unguarded touch here is a PROVEN race,
    /// `Tag::Unsafe`). `false` (`MutParam`) — an ordinary `mut` PARAMETER
    /// of a ONE-CALL scope; whether ITS specific argument at any given
    /// call site is exclusively-owned (a fresh scratch buffer threaded
    /// down a few hops) or genuinely shared is NOT decidable from the
    /// callee's signature alone — "cannot prove either way", `Tag::
    /// Undecided`, not a proven finding. Measured: `flush_out(mut tcp
    /// TcpStream, .., mut scratch []u8)` touching `scratch` — a `mut`
    /// PARAMETER, not `@` — was blanket `Tag::Unsafe` alongside genuine
    /// `@`-field races, collapsing two different confidence levels into
    /// one.
    is_self: bool,
}

fn place_root_is_self(root: &PlaceRoot) -> bool {
    matches!(root, PlaceRoot::SelfBare | PlaceRoot::SelfField(_))
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
            // Приёмка интегратора 2026-08-06 (четвёртый раунд) — measured
            // live gap: `TcpStream.rc` (wave 3 representation) is now
            // `*mut AtomicInt`, and `@share()`'s `@rc.fetch_add(1)` touches
            // a POINTER field — `is_mut_alias_safe`'s own poison-base rule
            // (`share_check.rs`: "a raw pointer, ANY pointee, never share
            // by structure — only the CONTAINING type's own `#share` vouch
            // escapes") refuses it unconditionally, deliberately, for ITS
            // OWN callers (the capture-check: a raw pointer CAPTURED by
            // reference has no static guarantee its pointee stays #share
            // for the alias's whole lifetime — that rule is right there).
            // `touch_share_ok` asks a NARROWER question — is calling a
            // method through THIS receiver's OWN field, whose declared
            // POINTEE type is statically known right here, safe — and for
            // that question a pointer to an audited `#share` type carries
            // the SAME vouch its pointee does: `*mut AtomicInt`'s pointee
            // IS `AtomicInt`, whose own atomic ops are its entire purpose.
            // Deref-through-pointer does not strip `#share`-ness, it is
            // just how the field happens to be stored.
            if pointee_share_ok(&t, ctx.type_decls) {
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

/// Приёмка интегратора 2026-08-06 (четвёртый раунд): `true` iff `ty` is a
/// raw pointer (`*T`/`*mut T`/`*ro T`/`*uninit T` — any pointee modifier,
/// strips the SAME way `typeref_named_base` does) whose POINTEE resolves
/// to a type declaration carrying the `#share` audited vouch
/// (`TypeAttr::Share`). Used ONLY by [`touch_share_ok`] — a narrower,
/// intentional widening of `is_mut_alias_safe`'s own stricter "raw
/// pointer is an unconditional poison base" rule, see that call site's
/// doc for why the two questions differ.
fn pointee_share_ok(ty: &TypeRef, type_decls: &HashMap<String, &TypeDecl>) -> bool {
    match ty {
        TypeRef::Readonly(inner, _) | TypeRef::Mut(inner, _) | TypeRef::Uninit(inner, _) => {
            pointee_share_ok(inner, type_decls)
        }
        TypeRef::Pointer(inner, _) => typeref_named_base(inner)
            .and_then(|b| type_decls.get(b))
            .map(|td| td.attrs.contains(&crate::ast::TypeAttr::Share))
            .unwrap_or(false),
        _ => false,
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

/// Plan 238 Ф.3 (D446 §4/§5 амендмент) — `true` iff `name` is a DECLARED
/// field, on the type reached at `root`, whose OWN type is function-typed
/// (`is_func_type`). Mirrors [`touch_share_ok`]'s own by-`PlaceRoot` type
/// lookup (`SelfBare`→receiver type itself, `SelfField`/`MutParam`→that
/// field's/param's declared type) but asks a DIFFERENT question of it —
/// see [`FnFacts::field_call_unresolved`]'s doc for why this matters (a
/// call through such a field is D446 §4's indirect-call territory, not a
/// proven touch, even though it is syntactically indistinguishable from an
/// unresolved method call in `pb_walk_expr`'s Call arm).
fn field_call_is_func_type(root: &PlaceRoot, name: &str, ctx: &PassBCtx) -> bool {
    let owner_ty: Option<TypeRef> = match root {
        PlaceRoot::SelfBare => ctx.receiver_type.map(|tn| TypeRef::Named {
            path: vec![tn.to_string()],
            generics: vec![],
            span: Span::default(),
        }),
        PlaceRoot::SelfField(f) => {
            ctx.receiver_type.and_then(|tn| ctx.type_decls.get(tn)).and_then(|td| field_type_of(td, f))
        }
        PlaceRoot::MutParam(p) => ctx.param_types.get(p).cloned(),
    };
    let Some(owner_ty) = owner_ty else { return false };
    let Some(base) = typeref_named_base(&owner_ty) else { return false };
    let Some(td) = ctx.type_decls.get(base) else { return false };
    field_type_of(td, name).map(|ft| is_func_type(&ft)).unwrap_or(false)
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
                    is_self: place_root_is_self(&root),
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
                        is_self: place_root_is_self(&root),
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
                    // Plan 238 Ф.3 (D446 §4/§5 амендмент) — see `FnFacts::
                    // field_call_unresolved`'s doc: an UNRESOLVED call whose
                    // `name` is a DECLARED, function-typed field on the
                    // reached type is a call THROUGH that field's value, not
                    // an unresolved METHOD — D446 §4's indirect-call
                    // territory (Undecided), not a proven `@`-touch (Unsafe).
                    // Checked BEFORE the conservative "unresolved ⇒ mutates"
                    // default below, which cannot make this distinction on
                    // its own.
                    if resolved.is_none() && field_call_is_func_type(&root, name, ctx) {
                        facts.field_call_unresolved = true;
                        pb_walk_expr(func, ctx, guards, facts);
                        pb_walk_call_args(e, args, trailing, ctx, guards, facts);
                        if let ExprKind::Member { obj, .. } = &func.kind {
                            if let ExprKind::Ident(name) = &obj.kind {
                                guards.retain(|g| g != name);
                            }
                        }
                        return;
                    }
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
                            is_self: place_root_is_self(&root),
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
        ExprKind::Supervised { body, cancel, deadline, on_timeout } => {
            if let Some(c) = cancel {
                pb_walk_expr(c, ctx, guards, facts);
            }
            if let Some(dl) = deadline {
                pb_walk_expr(&dl.expr, ctx, guards, facts);
            }
            if let Some(oh) = on_timeout {
                pb_walk_expr(oh, ctx, guards, facts);
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
    // Приёмка интегратора 2026-08-06 (третий раунд): a `@`/self-rooted
    // unguarded touch is a PROVEN finding (the receiver is persistent,
    // reachable-again state — `Tag::Unsafe`, always enforced). A `mut`-
    // PARAMETER-rooted one is the model's OWN limit — whether THIS
    // argument is exclusively-owned isn't decidable from the callee's
    // signature (`Tag::Undecided`, gated). Self-touches checked FIRST —
    // if a fn has BOTH kinds, the proven finding wins (worse tag).
    if facts.touches.iter().any(|t| !t.guarded && !t.share_ok && t.is_self) {
        return Intra::Fixed(FnSafety {
            tag: Tag::Unsafe,
            reason: "unguarded_mutation",
            detail: None,
            detail_span: None,
        });
    }
    if facts.touches.iter().any(|t| !t.guarded && !t.share_ok && !t.is_self) {
        return Intra::Fixed(FnSafety {
            tag: Tag::Undecided,
            reason: "unguarded_mut_param",
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
    // Plan 238 Ф.3 (D446 §4/§5 амендмент) — see `FnFacts::field_call_
    // unresolved`'s doc: a call through a function-typed FIELD is D446 §4's
    // indirect-call territory, model-limit Undecided, not a proven touch.
    if facts.field_call_unresolved {
        return Intra::Fixed(FnSafety {
            tag: Tag::Undecided,
            reason: "field_call_unresolved",
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
            // Приёмка интегратора 2026-08-06 (второй раунд): a GENERIC fn
            // is now the SAME fixed-point candidate as an ordinary one —
            // "Safe iff every RESOLVED callee is Safe" IS the fixed point
            // this whole graph already computes; the earlier "zero calls"
            // leaf rule was an unnecessary special case of exactly that
            // (owner's call — "не выдумывай спецслучай"). Falls through to
            // the shared `analyze_fn_body`/`facts_to_intra` path below,
            // UNCHANGED from the non-generic case; `candidates`/`local_
            // callees_of` do not know or care that a span came from a
            // "generic" signature. The residual, UNCLOSED risk this
            // accepts (same one the plan's own bound-sensitivity deferral
            // already named, D446 Ф.1 report §6 pt.4): a call dispatched
            // THROUGH the generic parameter's own protocol bound (`t.
            // display()` for `fn[T Printable]`) is invisible to `resolved_
            // callees` the SAME way any unresolvable indirect target is —
            // such a fn can settle Safe without that call ever being
            // counted. This is not NEW exposure specific to generics
            // (protocol/existential dispatch is already invisible to this
            // graph for non-generic code too); it is the owner-accepted
            // cost of closing the FAR more common "generic leaf/plumbing
            // fn calling only concrete, resolvable things" case.
            if reason != "generic" {
                final_tag.insert(
                    span,
                    FnSafety { tag: Tag::Undecided, reason, detail: None, detail_span: None },
                );
                continue;
            }
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
    // Plan 238 Ф.3 (D446 §4/§5 амендмент): fn-typed param NAMES of the
    // Item::Fn currently being walked — see `check_one_seed`'s use of
    // `own_param_names` for why (a call to one of THESE by bare name is no
    // longer this section's problem to flag; the requirement/passing-check
    // mechanism in `check_param_passing` covers it exactly, everywhere it
    // is actually passed an argument). Empty set for `Item::Test` (a test
    // body has no params of its own).
    let empty_param_names: HashSet<String> = HashSet::new();
    for item in &module.items {
        match item {
            Item::Fn(fd) => {
                let own_param_names: HashSet<String> =
                    fd.params.iter().filter(|p| is_func_type(&p.ty)).map(|p| p.name.clone()).collect();
                match &fd.body {
                    FnBody::Block(b) => find_seeds_block(
                        b, resolved_callees, tags, &fn_index, &name_index, &type_method_index,
                        &own_param_names, errors,
                    ),
                    FnBody::Expr(e) => find_seeds_expr(
                        e, resolved_callees, tags, &fn_index, &name_index, &type_method_index,
                        &own_param_names, errors,
                    ),
                    FnBody::External => {}
                }
            }
            Item::Test(td) => find_seeds_block(
                &td.body, resolved_callees, tags, &fn_index, &name_index, &type_method_index,
                &empty_param_names, errors,
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
    own_param_names: &HashSet<String>,
    errors: &mut Vec<crate::diag::Diagnostic>,
) {
    for s in &b.stmts {
        find_seeds_stmt(s, rc, tags, fn_index, name_index, type_method_index, own_param_names, errors);
    }
    if let Some(t) = &b.trailing {
        find_seeds_expr(t, rc, tags, fn_index, name_index, type_method_index, own_param_names, errors);
    }
}

fn find_seeds_stmt(
    s: &Stmt,
    rc: &HashMap<ExprId, Span>,
    tags: &HashMap<Span, FnSafety>,
    fn_index: &HashMap<Span, &FnDecl>,
    name_index: &HashMap<String, Vec<Span>>,
    type_method_index: &HashMap<(String, String), Vec<Span>>,
    own_param_names: &HashSet<String>,
    errors: &mut Vec<crate::diag::Diagnostic>,
) {
    match s {
        Stmt::Let(d) => find_seeds_expr(&d.value, rc, tags, fn_index, name_index, type_method_index, own_param_names, errors),
        Stmt::Expr(e) => find_seeds_expr(e, rc, tags, fn_index, name_index, type_method_index, own_param_names, errors),
        Stmt::Assign { target, value, .. } => {
            find_seeds_expr(target, rc, tags, fn_index, name_index, type_method_index, own_param_names, errors);
            find_seeds_expr(value, rc, tags, fn_index, name_index, type_method_index, own_param_names, errors);
        }
        Stmt::TupleAssign { lhs, rhs, .. } => {
            for e in lhs {
                find_seeds_expr(e, rc, tags, fn_index, name_index, type_method_index, own_param_names, errors);
            }
            for e in rhs {
                find_seeds_expr(e, rc, tags, fn_index, name_index, type_method_index, own_param_names, errors);
            }
        }
        Stmt::Return { value, .. } => {
            if let Some(v) = value {
                find_seeds_expr(v, rc, tags, fn_index, name_index, type_method_index, own_param_names, errors);
            }
        }
        Stmt::Throw { value, .. } => find_seeds_expr(value, rc, tags, fn_index, name_index, type_method_index, own_param_names, errors),
        Stmt::Defer { body, .. } => find_seeds_expr(body, rc, tags, fn_index, name_index, type_method_index, own_param_names, errors),
        Stmt::ConsumeScope { init, body, .. } => {
            find_seeds_expr(init, rc, tags, fn_index, name_index, type_method_index, own_param_names, errors);
            find_seeds_block(body, rc, tags, fn_index, name_index, type_method_index, own_param_names, errors);
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
    own_param_names: &HashSet<String>,
    errors: &mut Vec<crate::diag::Diagnostic>,
) {
    match &e.kind {
        ExprKind::Spawn(inner) => {
            let mut calls = Vec::new();
            let mut locals = HashSet::new();
            collect_seed_calls_expr(inner, &mut calls, &mut locals);
            check_one_seed("spawn", &calls, rc, tags, fn_index, name_index, type_method_index, own_param_names, errors);
            find_seeds_expr(inner, rc, tags, fn_index, name_index, type_method_index, own_param_names, errors);
        }
        ExprKind::Detach(body) => {
            let mut calls = Vec::new();
            let mut locals = HashSet::new();
            collect_seed_calls_block(body, &mut calls, &mut locals);
            check_one_seed("detach", &calls, rc, tags, fn_index, name_index, type_method_index, own_param_names, errors);
            find_seeds_block(body, rc, tags, fn_index, name_index, type_method_index, own_param_names, errors);
        }
        ExprKind::ParallelFor { iter, body, .. } => {
            find_seeds_expr(iter, rc, tags, fn_index, name_index, type_method_index, own_param_names, errors);
            let mut calls = Vec::new();
            let mut locals = HashSet::new();
            collect_seed_calls_block(body, &mut calls, &mut locals);
            check_one_seed("parallel for", &calls, rc, tags, fn_index, name_index, type_method_index, own_param_names, errors);
            find_seeds_block(body, rc, tags, fn_index, name_index, type_method_index, own_param_names, errors);
        }
        ExprKind::Blocking(b) => find_seeds_block(b, rc, tags, fn_index, name_index, type_method_index, own_param_names, errors),
        ExprKind::Call { func, args, trailing } => {
            find_seeds_expr(func, rc, tags, fn_index, name_index, type_method_index, own_param_names, errors);
            for a in args {
                find_seeds_expr(a.expr(), rc, tags, fn_index, name_index, type_method_index, own_param_names, errors);
            }
            if let Some(t) = trailing {
                match t {
                    Trailing::Block(b) => find_seeds_block(b, rc, tags, fn_index, name_index, type_method_index, own_param_names, errors),
                    Trailing::LegacyBlockWithParams(tb) => {
                        find_seeds_block(&tb.body, rc, tags, fn_index, name_index, type_method_index, own_param_names, errors)
                    }
                    Trailing::Fn(sb) => match &sb.body {
                        FnBody::Block(b) => find_seeds_block(b, rc, tags, fn_index, name_index, type_method_index, own_param_names, errors),
                        FnBody::Expr(ex) => find_seeds_expr(ex, rc, tags, fn_index, name_index, type_method_index, own_param_names, errors),
                        FnBody::External => {}
                    },
                }
            }
        }
        ExprKind::Lambda { body, .. } => find_seeds_expr(body, rc, tags, fn_index, name_index, type_method_index, own_param_names, errors),
        ExprKind::ClosureLight { body, .. } => match body {
            ClosureBody::Expr(ex) => find_seeds_expr(ex, rc, tags, fn_index, name_index, type_method_index, own_param_names, errors),
            ClosureBody::Block(b) => find_seeds_block(b, rc, tags, fn_index, name_index, type_method_index, own_param_names, errors),
        },
        ExprKind::ClosureFull(sb) => match &sb.body {
            FnBody::Block(b) => find_seeds_block(b, rc, tags, fn_index, name_index, type_method_index, own_param_names, errors),
            FnBody::Expr(ex) => find_seeds_expr(ex, rc, tags, fn_index, name_index, type_method_index, own_param_names, errors),
            FnBody::External => {}
        },
        ExprKind::Block(b) => find_seeds_block(b, rc, tags, fn_index, name_index, type_method_index, own_param_names, errors),
        ExprKind::If { cond, then, else_ } => {
            find_seeds_expr(cond, rc, tags, fn_index, name_index, type_method_index, own_param_names, errors);
            find_seeds_block(then, rc, tags, fn_index, name_index, type_method_index, own_param_names, errors);
            match else_ {
                Some(ElseBranch::Block(b)) => find_seeds_block(b, rc, tags, fn_index, name_index, type_method_index, own_param_names, errors),
                Some(ElseBranch::If(e2)) => find_seeds_expr(e2, rc, tags, fn_index, name_index, type_method_index, own_param_names, errors),
                None => {}
            }
        }
        ExprKind::IfLet { scrutinee, guard, then, else_, .. } => {
            find_seeds_expr(scrutinee, rc, tags, fn_index, name_index, type_method_index, own_param_names, errors);
            if let Some(g) = guard {
                find_seeds_expr(g, rc, tags, fn_index, name_index, type_method_index, own_param_names, errors);
            }
            find_seeds_block(then, rc, tags, fn_index, name_index, type_method_index, own_param_names, errors);
            match else_ {
                Some(ElseBranch::Block(b)) => find_seeds_block(b, rc, tags, fn_index, name_index, type_method_index, own_param_names, errors),
                Some(ElseBranch::If(e2)) => find_seeds_expr(e2, rc, tags, fn_index, name_index, type_method_index, own_param_names, errors),
                None => {}
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            find_seeds_expr(scrutinee, rc, tags, fn_index, name_index, type_method_index, own_param_names, errors);
            for a in arms {
                if let Some(g) = &a.guard {
                    find_seeds_expr(g, rc, tags, fn_index, name_index, type_method_index, own_param_names, errors);
                }
                match &a.body {
                    MatchArmBody::Expr(be) => find_seeds_expr(be, rc, tags, fn_index, name_index, type_method_index, own_param_names, errors),
                    MatchArmBody::Block(bb) => find_seeds_block(bb, rc, tags, fn_index, name_index, type_method_index, own_param_names, errors),
                }
            }
        }
        ExprKind::For { iter, body, .. } => {
            find_seeds_expr(iter, rc, tags, fn_index, name_index, type_method_index, own_param_names, errors);
            find_seeds_block(body, rc, tags, fn_index, name_index, type_method_index, own_param_names, errors);
        }
        ExprKind::While { cond, body, .. } => {
            find_seeds_expr(cond, rc, tags, fn_index, name_index, type_method_index, own_param_names, errors);
            find_seeds_block(body, rc, tags, fn_index, name_index, type_method_index, own_param_names, errors);
        }
        ExprKind::WhileLet { scrutinee, guard, body, .. } => {
            find_seeds_expr(scrutinee, rc, tags, fn_index, name_index, type_method_index, own_param_names, errors);
            if let Some(g) = guard {
                find_seeds_expr(g, rc, tags, fn_index, name_index, type_method_index, own_param_names, errors);
            }
            find_seeds_block(body, rc, tags, fn_index, name_index, type_method_index, own_param_names, errors);
        }
        ExprKind::Loop { body, .. } => find_seeds_block(body, rc, tags, fn_index, name_index, type_method_index, own_param_names, errors),
        ExprKind::Supervised { body, cancel, deadline, on_timeout } => {
            if let Some(c) = cancel {
                find_seeds_expr(c, rc, tags, fn_index, name_index, type_method_index, own_param_names, errors);
            }
            if let Some(dl) = deadline {
                find_seeds_expr(&dl.expr, rc, tags, fn_index, name_index, type_method_index, own_param_names, errors);
            }
            if let Some(oh) = on_timeout {
                find_seeds_expr(oh, rc, tags, fn_index, name_index, type_method_index, own_param_names, errors);
            }
            find_seeds_block(body, rc, tags, fn_index, name_index, type_method_index, own_param_names, errors);
        }
        ExprKind::With { bindings, body } => {
            for b in bindings {
                find_seeds_expr(&b.handler, rc, tags, fn_index, name_index, type_method_index, own_param_names, errors);
            }
            find_seeds_block(body, rc, tags, fn_index, name_index, type_method_index, own_param_names, errors);
        }
        ExprKind::Forbid { body, .. } | ExprKind::Realtime { body, .. } => {
            find_seeds_block(body, rc, tags, fn_index, name_index, type_method_index, own_param_names, errors)
        }
        ExprKind::Try(inner) | ExprKind::Bang(inner) | ExprKind::RefArg(inner) => {
            find_seeds_expr(inner, rc, tags, fn_index, name_index, type_method_index, own_param_names, errors)
        }
        ExprKind::Coalesce(a, b) => {
            find_seeds_expr(a, rc, tags, fn_index, name_index, type_method_index, own_param_names, errors);
            find_seeds_expr(b, rc, tags, fn_index, name_index, type_method_index, own_param_names, errors);
        }
        ExprKind::As(inner, _) | ExprKind::Is(inner, _) => {
            find_seeds_expr(inner, rc, tags, fn_index, name_index, type_method_index, own_param_names, errors)
        }
        ExprKind::Binary { left, right, .. } => {
            find_seeds_expr(left, rc, tags, fn_index, name_index, type_method_index, own_param_names, errors);
            find_seeds_expr(right, rc, tags, fn_index, name_index, type_method_index, own_param_names, errors);
        }
        ExprKind::Unary { operand, .. } => find_seeds_expr(operand, rc, tags, fn_index, name_index, type_method_index, own_param_names, errors),
        ExprKind::Member { obj, .. } => find_seeds_expr(obj, rc, tags, fn_index, name_index, type_method_index, own_param_names, errors),
        ExprKind::Index { obj, index } => {
            find_seeds_expr(obj, rc, tags, fn_index, name_index, type_method_index, own_param_names, errors);
            find_seeds_expr(index, rc, tags, fn_index, name_index, type_method_index, own_param_names, errors);
        }
        ExprKind::TupleLit(elems) => {
            for el in elems {
                find_seeds_expr(el, rc, tags, fn_index, name_index, type_method_index, own_param_names, errors);
            }
        }
        ExprKind::TurboFish { base, .. } => find_seeds_expr(base, rc, tags, fn_index, name_index, type_method_index, own_param_names, errors),
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
                // Приёмка интегратора 2026-08-06 (третий раунд) — companion
                // for a FREE-function call whose value-bearing arguments are
                // ALL seed-local (`write_response_keepalive(shs, resp)`
                // where both `shs`/`resp` are `let`-bound in this same
                // spawn body): the exact same "fresh, non-escaping" proof,
                // just for a free fn instead of a method receiver. `true`
                // only when EVERY argument is a bare local `Ident` (no
                // partial credit — a mixed call with even one non-local
                // arg stays unexempted, conservative).
                ExprKind::Ident(_) => !args.is_empty()
                    && args.iter().all(|a| match &a.expr().kind {
                        ExprKind::Ident(name) => locals.contains(name),
                        _ => false,
                    }),
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
        ExprKind::Supervised { body, cancel, deadline, on_timeout } => {
            if let Some(c) = cancel {
                collect_seed_calls_expr(c, out, locals);
            }
            if let Some(dl) = deadline {
                collect_seed_calls_expr(&dl.expr, out, locals);
            }
            if let Some(oh) = on_timeout {
                collect_seed_calls_expr(oh, out, locals);
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
        Stmt::ConsumeScope { binding, init, body, .. } => {
            // Приёмка интегратора 2026-08-06 (третий раунд): `spawn consume
            // shs = expr { body }` (D415 §4 move-capture — desugars to a
            // `ConsumeScope` wrapping the spawn body itself, measured live:
            // `nova-polaris/src/net/serve.nv`'s `spawn consume shs = s.
            // share() { .. write_response_keepalive(shs, resp) .. }`) binds
            // `binding` EVEN MORE safely-local than an ordinary `let` — an
            // explicit move guarantees single ownership by construction
            // (D415's whole point). Register it the SAME as a `Let`-bound
            // name so the free-fn `local_receiver` check (`collect_seed_
            // calls_expr`'s `ExprKind::Ident` arm) recognises it.
            locals.insert(binding.clone());
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
    own_param_names: &HashSet<String>,
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
                // Plan 238 Ф.3 (D446 §4/§5 амендмент): a call whose bare
                // name matches one of THIS Item::Fn's OWN function-typed
                // parameters is the base case `compute_required_params`
                // itself infers from THIS exact shape (rule 1 — called
                // directly inside a spawn/detach/parallel-for reached in
                // the declaring fn's own body). The real risk (an unsafe
                // closure reaching this parameter) is now checked at every
                // site that PASSES an argument into it
                // (`check_param_passing`, `E_FIBER_UNSAFE_ARG`) — flagging
                // the call to the PARAMETER itself here too would be a
                // duplicate, strictly-worse-worded finding for the exact
                // same risk (no chain to the actual unsafe capture, no
                // knowledge of which callers are even safe) — retracted
                // unconditionally, not just under the flag (this specific
                // shape no longer needs `NOVA_FIBER_INDIRECT=1` at all,
                // `PROGRESS-p238-f3.md` §2/§5).
                if call.name.as_deref().map(|n| own_param_names.contains(n)).unwrap_or(false) {
                    continue;
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
    // Приёмка интегратора 2026-08-06 (третий раунд) — the governing
    // principle, stated once here rather than as a pile of per-reason
    // special cases: the tag ITSELF already draws the right line.
    //
    //   `Tag::Unsafe`     — PROVEN unsafe: an actual unguarded touch
    //                       (`unguarded_mutation`), or a call chain that
    //                       transitively REACHES one (`calls_unsafe`).
    //                       Always enforced, flag or no flag — this is a
    //                       real, demonstrated race, not a limit of the
    //                       model.
    //   `Tag::Undecided`  — the MODEL'S limit, not a finding about the
    //                       code: `no_body`/`extern` (opaque FFI),
    //                       `fn_param_in_sig` (HOF wrapper — D446 §4's
    //                       "непрямые вызовы" territory, just reached by a
    //                       direct call to the wrapper), a generic fn whose
    //                       callee resolution hit a protocol-dispatch wall
    //                       (`calls_undecided`), `cross_module_callee`, or
    //                       (the `None`-tag branch below) a target this
    //                       compile unit never computed a tag for at all.
    //                       ALL of these say "cannot prove EITHER way", not
    //                       "proven unsafe" — gated behind `NOVA_FIBER_
    //                       INDIRECT`, the SAME bucket as an unresolved
    //                       indirect call (D446 §4), because they need the
    //                       SAME missing thing: a type-level safety marker
    //                       (or, for `cross_module_callee`, a cross-unit
    //                       channel) neither of which exists yet.
    //
    // `local_receiver`'s exemption (below) is a THIRD thing — not a gate
    // on an already-Unsafe/Undecided verdict, but proof the touch was
    // never actually unsafe in the first place (a fresh, non-escaping
    // receiver) — stays unconditional, flag-independent, same as before.
    match tags.get(&target) {
        // `local_receiver` — this call's receiver is a name bound by a
        // `Let` WITHIN this seed body (never captured). If the ENTIRE
        // reason `target` isn't `Safe` is its own DIRECT self-touch
        // (`unguarded_mutation` — reached its own `@`/mut-param, not a
        // further `calls_unsafe`/`calls_undecided` hop into something
        // else), that self-touch is happening on a receiver PROVABLY local
        // to this fiber — exactly as safe as this seed body mutating it
        // directly would be (Pass B's OWN "local mut accumulator is not a
        // touch" rule, `record_call_edge`'s twin fix, applied here at the
        // seed point where that fix could not reach). Measured live gap:
        // `Vec.resize`/`.push`/... on a freshly-`let`-bound local inside a
        // `spawn` body (`std/src/net/byte_surface_test.nv`) was blanket
        // `E_FIBER_UNSAFE_CALL` even though nothing here is shared at all.
        // Приёмка интегратора 2026-08-06 (третий раунд, финальная зачистка):
        // broadened from "only a DIRECT self-touch" to "any `Tag::Unsafe`
        // verdict" — measured live gap: `resp.header(..)` (`resp` fresh/
        // `let`-bound in the seed) calling `ServerResponse.header`, whose
        // OWN reason settles as `calls_unsafe` (NOT a direct touch — it
        // transitively reaches `HeaderMap.insert`'s self-touch on `resp`'s
        // OWN `@headers` field), was still rejected under the narrower
        // rule. Since the Ф.2 self/mut-param split (this same round) means
        // `Tag::Unsafe` is now NEVER produced by a mut-param touch (only by
        // a `@`/self-rooted one, `facts_to_intra`), every `Tag::Unsafe`
        // chain — direct or propagated — bottoms out in SOME receiver's
        // own field being mutated; when the OUTERMOST receiver at the seed
        // is provably fresh, that fresh object graph is what every nested
        // self-touch in the chain is reached through in the common case.
        // Accepted, undemonstrated residual risk: a callee that reaches an
        // UNRELATED shared object via a name OTHER than the traced
        // receiver (e.g. `resp.finalize()` internally touching an
        // unrelated global registry) would ALSO be silently exempted here
        // — no live instance of this shape has been measured; flagged
        // honestly in the report rather than solved (would need per-hop
        // receiver-provenance threading through the call graph — real
        // interprocedural work, out of this window's budget).
        Some(fs) if local_receiver && fs.tag == Tag::Unsafe => {}
        // Cross-compile-unit gap (Ф.1 report §6 point 3): this compile
        // unit never computed a tag for `target` at all — genuinely
        // "cannot prove either way", the Undecided family, gated the same.
        None => {
            if !indirect_enforcement_enabled() {
                return;
            }
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
        // `Tag::Undecided` — the model's limit, not a proven finding —
        // gated behind the SAME flag as an indirect call.
        Some(fs) if fs.tag == Tag::Undecided && !indirect_enforcement_enabled() => {}
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
        "unguarded_mut_param" => "mutates a `mut` PARAMETER (not `@`) with no live lock and no `#share`/`consume`-verified type — whether this specific argument is exclusively-owned isn't decidable from the signature alone",
        "no_body" => "bottoms out in an external (no-body) fn — nothing to prove from",
        "generic" => "bottoms out in a generic fn with no safety bound on its type parameter",
        "extern" => "bottoms out in an `extern`-declared fn",
        "fn_param_in_sig" => "bottoms out in a fn taking a function-typed parameter — its target is not statically closed",
        "cross_module_callee" => "bottoms out in a call whose target is outside this compile unit",
        "field_call_unresolved" => "bottoms out in a call through a struct field of function type — the concrete target is not statically known (D446 §4)",
        _ => "reason not recovered (defensive fallback — should not normally occur)",
    }
}

// ============================================================================
// Plan 238 Ф.3 (D446 §4/§5 амендмент, owner decision 2026-08-06): AUTOMATIC
// inference of a function-typed PARAMETER's safety REQUIREMENT, and exact
// enforcement at the point the argument is PASSED. Ф.1/Ф.2 built a total
// tag over ordinary functions/methods and enforced it at fiber-seeding
// points; this section is the missing per-PARAMETER axis: `fn_param_in_sig`
// (retracted above, `signature_undecided_reason`'s doc) used to blanket-gate
// EVERY function taking a closure, regardless of what it does with it, and
// left the actual risk (an unsafe closure crossing INTO a required
// parameter) invisible to the checker entirely — reachable only under
// `NOVA_FIBER_INDIRECT=1`, off by default, i.e. a silent hole matching
// exactly the owner's stated worry ("default without a marker is a big
// hole — a rare user will remember it").
//
// ── Inference (two rules, one fixed point) ─────────────────────────────
// 1. DIRECT: a function-typed parameter `p` of fn `F` is required if `F`'s
//    OWN body calls `p()` — a bare `Ident` call of `p`'s exact name —
//    anywhere reached SYNCHRONOUSLY inside a `spawn`/`detach`/`parallel
//    for` node in `F`'s body (any nesting depth, including through nested
//    closures literally written there — the SAME "in-fiber" extent
//    `check_seed_points`'s own seed-body walk already recognises,
//    `rp_walk_*` below mirrors `find_seeds_*`'s full `ExprKind` coverage).
//    `Blocking` is NOT a fiber boundary of its own (matches `find_seeds_
//    expr`'s existing stance — it schedules onto a blocking-safe thread
//    pool, it does not fork a fiber), so it does not flip the in-fiber
//    flag; it is walked through with whatever flag state it already had.
// 2. FORWARD: if `F`'s body passes ITS OWN parameter `p`, by bare name
//    UNCHANGED (no wrapping — same "visible by syntax alone" discipline
//    `compute_guarded_params`/`arg_root_safe` already use throughout this
//    file), as the argument at position `j` of a RESOLVED, LOCAL callee
//    `G`, and `G`'s OWN parameter `j` is (transitively) required, then `p`
//    is required too — this is the `middleware(f) → helper(g)` chain from
//    the owner's example: `helper`'s `g` is required by rule 1 (`helper`
//    calls `g()` directly inside its own `spawn`), `middleware`'s `f` is
//    required by rule 2 (forwarded, unchanged, into `helper`'s `g`).
// Both rules run together as ONE fixed point over `(fn Span, param index)`
// nodes — [`compute_required_params`] — mirroring [`run`]'s own tag
// fixed-point (same "propagate along a graph until nothing changes"
// shape, a DIFFERENT graph/axis: "does this parameter need a Safe
// closure", not "is this function itself Safe"). Default is safe in BOTH
// directions (П.2/П.3's own governing discipline, reapplied to a new
// axis): a parameter nothing marks required stays UNCHECKED (no new
// diagnostic, ever); a parameter the fixed point DOES mark required is
// checked at EVERY call site that passes it an argument, program-wide —
// no "marked once, forgotten" escape hatch.
//
// ── `#fiber_safe` — the ONLY case inference cannot reach ───────────────
// A `no_body`/`extern` fn's parameters have no visible body to infer
// FROM — rule 1 can never fire for them. `#fiber_safe` (parsed on ANY
// param, `parser/mod.rs`'s `parse_fiber_safe_attr`, mirrors `#cancel_safe`
// exactly) is the explicit escape hatch for exactly that boundary: "I
// (the FFI author) attest this parameter is invoked in a context that
// needs a Safe closure — trust me, the compiler cannot see why." Accepted
// as an ADDITIONAL, unconditional base case regardless of whether the fn
// has a body too (a strictly conservative no-op there — "requires more,
// never less").
//
// ── Enforcement — [`check_param_passing`] ──────────────────────────────
// Walks EVERY call site in the WHOLE module (not just seed bodies — the
// risk crystallises wherever the argument is PASSED, which need not be
// textually inside a spawn at all; the closure might not run until deep
// inside the callee, possibly nested fibers away). At a call whose target
// has a required parameter at position `j`, the argument is resolved to a
// closure LITERAL — directly, or through ONE hop of a same-function `let
// name = <closure literal>` binding (`resolve_arg_closure` — "captures
// visible" per the owner's own phrasing: a further-indirected value (a
// parameter, a field, a name from another function) is NOT traced, stays
// silently unchecked by THIS mechanism specifically — D446 §4's existing,
// separate indirect-call machinery is untouched and still covers a
// genuinely opaque argument reaching a seed point directly). The
// closure's OWN safety is computed the SAME way Ф.1 computes any
// function's tag ([`tag_from_facts_and_callees`], reusing `facts_to_intra`
// verbatim) — but over the CLOSURE's captures instead of a receiver: any
// captured name that is a `mut` PARAMETER *or* `mut`-LOCAL of the
// ENCLOSING function (`collect_capturable_mut` — unlike an ordinary
// function-local `mut` accumulator, which this file's Ф.1 doc already
// explains is NOT a touch because it never escapes one call, a variable
// CAPTURED into a closure that is then handed to a required parameter is
// escaping BY CONSTRUCTION — the whole reason it needed capturing) is
// treated as PROVEN-reachable state (`Touch.is_self` forced `true` post-
// walk, `closure_own_facts`) — the SAME confidence class Ф.2's `facts_to_
// intra` already reserves for a receiver's own `@` field, not the softer
// "cannot prove either way" class an ordinary `mut`-PARAMETER touch gets
// for a ROUTINE function (D446 §4 Ф.2's `unguarded_mut_param` — see
// `PROGRESS-p238-f2.md` §4's own distinction; capture is a different,
// harder-evidenced question than "is this one argument fresh").
// `Safe` closures pass silently; anything else is `E_FIBER_UNSAFE_ARG`,
// new code, NOT gated behind `NOVA_FIBER_INDIRECT` (this is an EXACT
// check, not a conservative-by-necessity one — the argument's shape is
// syntactically known at the point of the diagnostic), carrying BOTH the
// "why does the PARAMETER require" chain ([`RequiredParams::render_chain`])
// and, when the closure's own unsafety is itself a further graph hop
// (`calls_unsafe`/`calls_undecided`), the existing `render_chain` for that
// too.
//
// ── What stays under `NOVA_FIBER_INDIRECT` after this section ─────────
// See `PROGRESS-p238-f3.md` §5 for the full, measured remainder — in
// summary, everything this inference structurally cannot see a NAME for:
// a call through a STRUCT FIELD of function type (`self.handler(..)` —
// `place_root`/`resolve_arg_closover` only ever look at bare `Ident`
// parameters and bare `Ident` call targets, never a field path), a
// multi-hop `mut []T` scratch buffer (`unguarded_mut_param`, unrelated to
// this axis entirely — a plain data buffer, not a closure), and a closure
// value reached through more than the ONE same-function `let`-hop
// `resolve_arg_closure` traces (a parameter re-forwarded as a bare
// argument IS covered — that is exactly rule 2 above — but a value
// stored in a COLLECTION, returned from ANOTHER function, or threaded
// through two or more `let`s is not).
// ============================================================================

/// Why `(fn Span, param index)` is required — enough to render the
/// backward "why does this parameter require a Safe closure" chain
/// ([`RequiredParams::render_chain`]), mirroring how [`FnSafety`]'s own
/// `detail`/`detail_span` let [`render_chain`] walk `calls_unsafe`/
/// `calls_undecided` links on the ORIGINAL (Ф.1/Ф.2) tag axis.
#[derive(Clone, Copy, Debug)]
enum ReqReason {
    /// Rule 1 — called directly, by bare name, inside a `spawn`/`detach`/
    /// `parallel for` node reached anywhere in the declaring fn's own body.
    DirectSeedCall,
    /// `#fiber_safe` explicit annotation on the parameter itself.
    ExplicitAttr,
    /// Rule 2 — this fn passes ITS OWN parameter, unchanged, as the
    /// argument at `(Span, usize)`'s parameter position, and THAT
    /// parameter is (transitively) required too.
    Forward(Span, usize),
}

/// Result of [`compute_required_params`] — which `(fn Span, param index)`
/// pairs require a `Safe` closure, and why (for chain rendering).
pub struct RequiredParams {
    reasons: HashMap<(Span, usize), ReqReason>,
}

impl RequiredParams {
    pub fn is_required(&self, span: Span, idx: usize) -> bool {
        self.reasons.contains_key(&(span, idx))
    }

    /// Ш.7-twin for the requirement axis (mirrors [`render_chain`]): walks
    /// `Forward` links from `start` to a terminal `DirectSeedCall`/
    /// `ExplicitAttr`, rendering `` `Fn`'s parameter `p` → … ``. Capped at
    /// 32 hops — same defensive slack as `render_chain` (the forward-edge
    /// graph this walks is acyclic by construction: a node's `reasons`
    /// entry, once set by the fixed point in `compute_required_params`, is
    /// never revisited/overwritten — `or_insert`/first-writer-wins).
    pub fn render_chain(&self, start: (Span, usize), fn_index: &HashMap<Span, &FnDecl>) -> String {
        let mut hops: Vec<String> = Vec::new();
        let mut cur = start;
        for _ in 0..32 {
            let pname = fn_index
                .get(&cur.0)
                .and_then(|fd| fd.params.get(cur.1))
                .map(|p| p.name.as_str())
                .unwrap_or("?");
            let fname = fn_index.get(&cur.0).map(|fd| render_fn_name(fd)).unwrap_or_default();
            hops.push(format!("`{}`'s parameter `{}`", fname, pname));
            match self.reasons.get(&cur) {
                Some(ReqReason::Forward(ns, ni)) => cur = (*ns, *ni),
                Some(ReqReason::DirectSeedCall) => {
                    hops.push("called directly inside a spawn/detach/parallel-for body".to_string());
                    break;
                }
                Some(ReqReason::ExplicitAttr) => {
                    hops.push("explicitly annotated `#fiber_safe`".to_string());
                    break;
                }
                None => break,
            }
        }
        hops.join(" → ")
    }
}

/// Per-fn scratch collected by [`rp_walk_expr`]/[`rp_walk_block`] — kept
/// separate from [`FnFacts`] (Ф.1/Ф.2's per-fn touch/edge facts) on
/// purpose: this is a DIFFERENT question ("which of MY OWN params get
/// called-in-fiber or forwarded"), computed by its OWN dedicated walk, not
/// bolted onto Pass B's already-large `pb_walk_expr` (keeps each walk's
/// job legible — same "one function, one job" precedent `compute_guarded_
/// params` (Pass A) already set alongside Pass B).
#[derive(Default)]
struct ReqFacts {
    /// Rule 1 hits — indices of THIS fn's own function-typed params
    /// called directly by name while `in_fiber`.
    direct: HashSet<usize>,
    /// Rule 2 hits — `(this fn's own param index, resolved local callee
    /// span, that callee's param index)` for every bare-name forward.
    forwards: Vec<(usize, Span, usize)>,
}

struct ReqCtx<'a> {
    param_index: &'a HashMap<&'a str, usize>,
    resolved_callees: &'a HashMap<ExprId, Span>,
    fn_index: &'a HashMap<Span, &'a FnDecl>,
}

/// П.4-twin: forwarding is checked at EVERY call, `in_fiber` or not — a
/// parameter forwarded from ordinary (non-fiber) code into a callee that
/// itself calls it in a fiber is exactly the `middleware`/`helper` chain
/// the owner's example names; only rule 1 (the DIRECT call) needs the
/// `in_fiber` flag.
fn rp_walk_block(b: &Block, ctx: &ReqCtx, in_fiber: bool, out: &mut ReqFacts) {
    for s in &b.stmts {
        rp_walk_stmt(s, ctx, in_fiber, out);
    }
    if let Some(t) = &b.trailing {
        rp_walk_expr(t, ctx, in_fiber, out);
    }
}

fn rp_walk_stmt(s: &Stmt, ctx: &ReqCtx, in_fiber: bool, out: &mut ReqFacts) {
    match s {
        Stmt::Let(d) => rp_walk_expr(&d.value, ctx, in_fiber, out),
        Stmt::Expr(e) => rp_walk_expr(e, ctx, in_fiber, out),
        Stmt::Assign { target, value, .. } => {
            rp_walk_expr(target, ctx, in_fiber, out);
            rp_walk_expr(value, ctx, in_fiber, out);
        }
        Stmt::TupleAssign { lhs, rhs, .. } => {
            for e in lhs {
                rp_walk_expr(e, ctx, in_fiber, out);
            }
            for e in rhs {
                rp_walk_expr(e, ctx, in_fiber, out);
            }
        }
        Stmt::Return { value, .. } => {
            if let Some(v) = value {
                rp_walk_expr(v, ctx, in_fiber, out);
            }
        }
        Stmt::Throw { value, .. } => rp_walk_expr(value, ctx, in_fiber, out),
        Stmt::Defer { body, .. } => rp_walk_expr(body, ctx, in_fiber, out),
        Stmt::ConsumeScope { init, body, .. } => {
            rp_walk_expr(init, ctx, in_fiber, out);
            rp_walk_block(body, ctx, in_fiber, out);
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

fn rp_walk_expr(e: &Expr, ctx: &ReqCtx, in_fiber: bool, out: &mut ReqFacts) {
    match &e.kind {
        ExprKind::Call { func, args, trailing } => {
            if in_fiber {
                if let ExprKind::Ident(name) = &func.kind {
                    if let Some(&idx) = ctx.param_index.get(name.as_str()) {
                        out.direct.insert(idx);
                    }
                }
            }
            // Rule 2 — regardless of `in_fiber` (see fn doc).
            if let Some(target) = ctx.resolved_callees.get(&e.id).copied() {
                if let Some(target_fd) = ctx.fn_index.get(&target) {
                    for (j, a) in args.iter().enumerate() {
                        if let ExprKind::Ident(name) = &a.expr().kind {
                            if let Some(&i) = ctx.param_index.get(name.as_str()) {
                                if target_fd.params.get(j).map(|p| is_func_type(&p.ty)).unwrap_or(false) {
                                    out.forwards.push((i, target, j));
                                }
                            }
                        }
                    }
                }
            }
            rp_walk_expr(func, ctx, in_fiber, out);
            for a in args {
                rp_walk_expr(a.expr(), ctx, in_fiber, out);
            }
            if let Some(t) = trailing {
                match t {
                    Trailing::Block(b) => rp_walk_block(b, ctx, in_fiber, out),
                    Trailing::LegacyBlockWithParams(tb) => rp_walk_block(&tb.body, ctx, in_fiber, out),
                    Trailing::Fn(sb) => match &sb.body {
                        FnBody::Block(b) => rp_walk_block(b, ctx, in_fiber, out),
                        FnBody::Expr(ex) => rp_walk_expr(ex, ctx, in_fiber, out),
                        FnBody::External => {}
                    },
                }
            }
        }
        ExprKind::Spawn(inner) => rp_walk_expr(inner, ctx, true, out),
        ExprKind::Detach(b) => rp_walk_block(b, ctx, true, out),
        // Not a fiber boundary of its own — see module doc/`find_seeds_expr`.
        ExprKind::Blocking(b) => rp_walk_block(b, ctx, in_fiber, out),
        ExprKind::ParallelFor { iter, body, .. } => {
            rp_walk_expr(iter, ctx, in_fiber, out);
            rp_walk_block(body, ctx, true, out);
        }
        ExprKind::Lambda { body, .. } => rp_walk_expr(body, ctx, in_fiber, out),
        ExprKind::ClosureLight { body, .. } => match body {
            ClosureBody::Expr(ex) => rp_walk_expr(ex, ctx, in_fiber, out),
            ClosureBody::Block(b) => rp_walk_block(b, ctx, in_fiber, out),
        },
        ExprKind::ClosureFull(sb) => match &sb.body {
            FnBody::Block(b) => rp_walk_block(b, ctx, in_fiber, out),
            FnBody::Expr(ex) => rp_walk_expr(ex, ctx, in_fiber, out),
            FnBody::External => {}
        },
        ExprKind::Block(b) => rp_walk_block(b, ctx, in_fiber, out),
        ExprKind::If { cond, then, else_ } => {
            rp_walk_expr(cond, ctx, in_fiber, out);
            rp_walk_block(then, ctx, in_fiber, out);
            match else_ {
                Some(ElseBranch::Block(b)) => rp_walk_block(b, ctx, in_fiber, out),
                Some(ElseBranch::If(e2)) => rp_walk_expr(e2, ctx, in_fiber, out),
                None => {}
            }
        }
        ExprKind::IfLet { scrutinee, guard, then, else_, .. } => {
            rp_walk_expr(scrutinee, ctx, in_fiber, out);
            if let Some(g) = guard {
                rp_walk_expr(g, ctx, in_fiber, out);
            }
            rp_walk_block(then, ctx, in_fiber, out);
            match else_ {
                Some(ElseBranch::Block(b)) => rp_walk_block(b, ctx, in_fiber, out),
                Some(ElseBranch::If(e2)) => rp_walk_expr(e2, ctx, in_fiber, out),
                None => {}
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            rp_walk_expr(scrutinee, ctx, in_fiber, out);
            for a in arms {
                if let Some(g) = &a.guard {
                    rp_walk_expr(g, ctx, in_fiber, out);
                }
                match &a.body {
                    MatchArmBody::Expr(be) => rp_walk_expr(be, ctx, in_fiber, out),
                    MatchArmBody::Block(bb) => rp_walk_block(bb, ctx, in_fiber, out),
                }
            }
        }
        ExprKind::For { iter, body, .. } => {
            rp_walk_expr(iter, ctx, in_fiber, out);
            rp_walk_block(body, ctx, in_fiber, out);
        }
        ExprKind::While { cond, body, .. } => {
            rp_walk_expr(cond, ctx, in_fiber, out);
            rp_walk_block(body, ctx, in_fiber, out);
        }
        ExprKind::WhileLet { scrutinee, guard, body, .. } => {
            rp_walk_expr(scrutinee, ctx, in_fiber, out);
            if let Some(g) = guard {
                rp_walk_expr(g, ctx, in_fiber, out);
            }
            rp_walk_block(body, ctx, in_fiber, out);
        }
        ExprKind::Loop { body, .. } => rp_walk_block(body, ctx, in_fiber, out),
        ExprKind::Supervised { body, cancel, deadline, on_timeout } => {
            if let Some(c) = cancel {
                rp_walk_expr(c, ctx, in_fiber, out);
            }
            if let Some(dl) = deadline {
                rp_walk_expr(&dl.expr, ctx, in_fiber, out);
            }
            if let Some(oh) = on_timeout {
                rp_walk_expr(oh, ctx, in_fiber, out);
            }
            rp_walk_block(body, ctx, in_fiber, out);
        }
        ExprKind::With { bindings, body } => {
            for b in bindings {
                rp_walk_expr(&b.handler, ctx, in_fiber, out);
            }
            rp_walk_block(body, ctx, in_fiber, out);
        }
        ExprKind::Forbid { body, .. } | ExprKind::Realtime { body, .. } => {
            rp_walk_block(body, ctx, in_fiber, out)
        }
        ExprKind::Try(inner) | ExprKind::Bang(inner) | ExprKind::RefArg(inner) => {
            rp_walk_expr(inner, ctx, in_fiber, out)
        }
        ExprKind::Coalesce(a, b) => {
            rp_walk_expr(a, ctx, in_fiber, out);
            rp_walk_expr(b, ctx, in_fiber, out);
        }
        ExprKind::As(inner, _) | ExprKind::Is(inner, _) => rp_walk_expr(inner, ctx, in_fiber, out),
        ExprKind::Binary { left, right, .. } => {
            rp_walk_expr(left, ctx, in_fiber, out);
            rp_walk_expr(right, ctx, in_fiber, out);
        }
        ExprKind::Unary { operand, .. } => rp_walk_expr(operand, ctx, in_fiber, out),
        ExprKind::Member { obj, .. } => rp_walk_expr(obj, ctx, in_fiber, out),
        ExprKind::Index { obj, index } => {
            rp_walk_expr(obj, ctx, in_fiber, out);
            rp_walk_expr(index, ctx, in_fiber, out);
        }
        ExprKind::TupleLit(elems) => {
            for el in elems {
                rp_walk_expr(el, ctx, in_fiber, out);
            }
        }
        ExprKind::TurboFish { base, .. } => rp_walk_expr(base, ctx, in_fiber, out),
        _ => {}
    }
}

/// Entry point: computes [`RequiredParams`] for every function-typed
/// parameter in `module`. Called ONCE per compile unit, alongside [`run`]
/// (same `resolved_callees` input, same one-module-at-a-time scope — see
/// [`run`]'s own module doc on the cross-module boundary; forwarding
/// through a call whose target is OUTSIDE this module's own `fn_index` is
/// simply invisible here, same conservative "cannot see past the
/// boundary" limit Ф.1 already documents).
pub fn compute_required_params(module: &Module, resolved_callees: &HashMap<ExprId, Span>) -> RequiredParams {
    let fn_index = build_fn_index(module);
    let mut reasons: HashMap<(Span, usize), ReqReason> = HashMap::new();
    let mut forward_edges: Vec<((Span, usize), (Span, usize))> = Vec::new();

    for (&span, fd) in &fn_index {
        for (i, p) in fd.params.iter().enumerate() {
            if p.fiber_safe_attr && is_func_type(&p.ty) {
                reasons.entry((span, i)).or_insert(ReqReason::ExplicitAttr);
            }
        }
        if fd.params.is_empty() {
            continue;
        }
        let param_index: HashMap<&str, usize> = fd
            .params
            .iter()
            .enumerate()
            .filter(|(_, p)| is_func_type(&p.ty))
            .map(|(i, p)| (p.name.as_str(), i))
            .collect();
        if param_index.is_empty() {
            continue;
        }
        let ctx = ReqCtx { param_index: &param_index, resolved_callees, fn_index: &fn_index };
        let mut facts = ReqFacts::default();
        match &fd.body {
            FnBody::Block(b) => rp_walk_block(b, &ctx, false, &mut facts),
            FnBody::Expr(e) => rp_walk_expr(e, &ctx, false, &mut facts),
            FnBody::External => {}
        }
        for idx in facts.direct {
            reasons.entry((span, idx)).or_insert(ReqReason::DirectSeedCall);
        }
        for (i, target, j) in facts.forwards {
            forward_edges.push(((span, i), (target, j)));
        }
    }

    // Fixed point — propagate required-ness backward along forward edges
    // until nothing changes (finite node set, monotone growth, mirrors
    // `run`'s own dataflow loop shape).
    let mut changed = true;
    while changed {
        changed = false;
        for &(src, dst) in &forward_edges {
            if !reasons.contains_key(&src) && reasons.contains_key(&dst) {
                reasons.insert(src, ReqReason::Forward(dst.0, dst.1));
                changed = true;
            }
        }
    }

    RequiredParams { reasons }
}

/// Shared by [`run`]'s own fixed point (conceptually — NOT refactored to
/// call this, to avoid touching `run`'s already-accepted, tested loop for
/// an unrelated change) and by [`closure_own_facts`]'s tag computation
/// below: given `facts` and the ALREADY-SETTLED `tags` map, resolve the
/// `Intra::CandidateSafe` case by taking the worst tag among `facts.local_
/// callees`. A closure literal is never itself a fixed-point PARTICIPANT
/// (it has no `Item::Fn` span, cannot be waited on by anything) — its own
/// `local_callees` are ordinary, already-settled `Item::Fn`s by the time
/// [`check_param_passing`] runs (after `run` completes), so no iteration
/// is needed here, a single pass over `facts.local_callees` suffices.
fn tag_from_facts_and_callees(
    facts: &FnFacts,
    tags: &HashMap<Span, FnSafety>,
    fn_index: &HashMap<Span, &FnDecl>,
) -> FnSafety {
    match facts_to_intra(facts) {
        Intra::Fixed(fs) => fs,
        Intra::CandidateSafe(reason) => {
            let mut worst = Tag::Safe;
            let mut worst_detail: Option<(&'static str, Span)> = None;
            for c in &facts.local_callees {
                // A callee this compile unit never tagged at all (should
                // not normally happen for a LOCAL `fn_index` member — `run`
                // tags every one — defensive only) is treated the same
                // conservative way as a genuine cross-module gap.
                let ctag = tags.get(c).map(|fs| fs.tag).unwrap_or(Tag::Undecided);
                if ctag.rank() > worst.rank() {
                    worst = ctag;
                    let r = if ctag == Tag::Unsafe { "calls_unsafe" } else { "calls_undecided" };
                    worst_detail = Some((r, *c));
                }
            }
            match worst {
                Tag::Safe => FnSafety { tag: Tag::Safe, reason, detail: None, detail_span: None },
                _ => {
                    let (r, callee_span) = worst_detail.unwrap();
                    let name = fn_index.get(&callee_span).map(|fd| render_fn_name(fd)).unwrap_or_default();
                    FnSafety { tag: worst, reason: r, detail: Some(name), detail_span: Some(callee_span) }
                }
            }
        }
    }
}

/// Collects, for `fd`, the set of names a closure literal defined INSIDE
/// `fd`'s body could capture and, if mutated, escape THIS one call: `mut`
/// parameters (as Ф.1/Ф.2 already track) PLUS `mut`-LOCAL `let` bindings
/// (which Ф.1/Ф.2 deliberately treat as "not a touch" for an ORDINARY
/// function — module doc's "purely local mut accumulators never escape
/// one call" — but a closure specifically CAPTURING one, then handed
/// onward to a required parameter, is the exact escape that reasoning
/// exempts elsewhere). Does NOT descend into a NESTED closure literal's
/// own body — that closure's own locals are a separate scope, not
/// capturable by another closure. Best-effort, flat (not block-scope-
/// precise — a shadowed name in an inner block is over-approximated as
/// "still capturable", the safe direction, same precedent as `collect_
/// pattern_names`'s own doc).
fn collect_capturable_mut(fd: &FnDecl, resolved_types: &HashMap<ExprId, super::ResolvedType>) -> (HashSet<String>, HashMap<String, TypeRef>) {
    let mut names: HashSet<String> = fd.params.iter().filter(|p| p.is_mut).map(|p| p.name.clone()).collect();
    let mut types: HashMap<String, TypeRef> = fd.params.iter().map(|p| (p.name.clone(), p.ty.clone())).collect();
    match &fd.body {
        FnBody::Block(b) => ccm_walk_block(b, &mut names, &mut types, resolved_types),
        FnBody::Expr(e) => ccm_walk_expr(e, &mut names, &mut types, resolved_types),
        FnBody::External => {}
    }
    (names, types)
}

fn ccm_note_let(d: &crate::ast::LetDecl, names: &mut HashSet<String>, types: &mut HashMap<String, TypeRef>, resolved_types: &HashMap<ExprId, super::ResolvedType>) {
    if d.mutable {
        if let Pattern::Ident { name, .. } = &d.pattern {
            names.insert(name.clone());
            if let Some(t) = &d.ty {
                types.insert(name.clone(), t.clone());
            } else if let Some(named) = resolved_type_named_ref(resolved_types.get(&d.value.id)) {
                // Приёмка Ф.3 (измеренный ложняк, `PROGRESS-p238-f3.md` §5):
                // idiomatic Nova almost never writes `mut x T = expr` with an
                // EXPLICIT annotation — the type is inferred from the RHS
                // (`mut counter = AtomicInt.new(0)`). Without this, EVERY
                // untyped `mut` local was invisible to `touch_share_ok`'s type
                // lookup (`ctx.param_types.get(name)` → `None` → NOT share-ok
                // → forced `Tag::Unsafe`) even when its inferred type is
                // `#share`-verified — measured live gap:
                // `standalone/mut_capture_transitive_atomic_control_test.nv`'s
                // OWN control case (a closure closing over `mut counter =
                // AtomicInt.new(0)`, passed BY VALUE into a spawn-invoking
                // parameter — the exact legal shape the fixture's own name
                // promises) went from silently-legal to a false `E_FIBER_
                // UNSAFE_ARG` before this fix. Sourced from the SAME checker
                // channel `resolved_callees` already is (§0/196) —
                // `resolved_types_buf`, keyed by the `let`'s OWN value
                // expression id (`d.value.id`), not a re-derive.
                types.insert(name.clone(), named);
            }
        }
    }
}

/// Extracts a synthetic `TypeRef::Named{path: [name], ..}` from a resolved
/// type's `Named` variant (peeling `Readonly`, mirroring `peel_view`) — the
/// SAME synthetic-carrier shape [`touch_share_ok`] already builds for
/// `PlaceRoot::SelfBare`. `None` for every other `ResolvedType` shape
/// (scalars, generics, ...) — those either need no `#share` check (`is_mut_
/// alias_safe`'s own primitive handling) or are genuinely out of reach here;
/// declining is the safe direction (falls back to the pre-existing
/// "type unknown ⇒ not share-ok" default, never a false Safe).
fn resolved_type_named_ref(rt: Option<&super::ResolvedType>) -> Option<TypeRef> {
    match rt? {
        super::ResolvedType::Readonly(inner) => resolved_type_named_ref(Some(inner)),
        super::ResolvedType::Named { name, .. } => {
            Some(TypeRef::Named { path: vec![name.clone()], generics: vec![], span: Span::default() })
        }
        _ => None,
    }
}

fn ccm_walk_block(b: &Block, names: &mut HashSet<String>, types: &mut HashMap<String, TypeRef>, resolved_types: &HashMap<ExprId, super::ResolvedType>) {
    for s in &b.stmts {
        ccm_walk_stmt(s, names, types, resolved_types);
    }
    if let Some(t) = &b.trailing {
        ccm_walk_expr(t, names, types, resolved_types);
    }
}

fn ccm_walk_stmt(s: &Stmt, names: &mut HashSet<String>, types: &mut HashMap<String, TypeRef>, resolved_types: &HashMap<ExprId, super::ResolvedType>) {
    match s {
        Stmt::Let(d) => {
            ccm_walk_expr(&d.value, names, types, resolved_types);
            ccm_note_let(d, names, types, resolved_types);
        }
        Stmt::Expr(e) => ccm_walk_expr(e, names, types, resolved_types),
        Stmt::Assign { target, value, .. } => {
            ccm_walk_expr(target, names, types, resolved_types);
            ccm_walk_expr(value, names, types, resolved_types);
        }
        Stmt::TupleAssign { lhs, rhs, .. } => {
            for e in lhs {
                ccm_walk_expr(e, names, types, resolved_types);
            }
            for e in rhs {
                ccm_walk_expr(e, names, types, resolved_types);
            }
        }
        Stmt::Defer { body, .. } => ccm_walk_expr(body, names, types, resolved_types),
        Stmt::ConsumeScope { init, body, .. } => {
            ccm_walk_expr(init, names, types, resolved_types);
            ccm_walk_block(body, names, types, resolved_types);
        }
        _ => {}
    }
}

/// Only descends into constructs that share `fd`'s OWN lexical scope
/// (control flow); a closure literal's body is its own scope, deliberately
/// NOT walked (see [`collect_capturable_mut`]'s doc).
fn ccm_walk_expr(e: &Expr, names: &mut HashSet<String>, types: &mut HashMap<String, TypeRef>, resolved_types: &HashMap<ExprId, super::ResolvedType>) {
    match &e.kind {
        ExprKind::Block(b) => ccm_walk_block(b, names, types, resolved_types),
        ExprKind::If { cond, then, else_ } => {
            ccm_walk_expr(cond, names, types, resolved_types);
            ccm_walk_block(then, names, types, resolved_types);
            match else_ {
                Some(ElseBranch::Block(b)) => ccm_walk_block(b, names, types, resolved_types),
                Some(ElseBranch::If(e2)) => ccm_walk_expr(e2, names, types, resolved_types),
                None => {}
            }
        }
        ExprKind::IfLet { scrutinee, then, else_, .. } => {
            ccm_walk_expr(scrutinee, names, types, resolved_types);
            ccm_walk_block(then, names, types, resolved_types);
            match else_ {
                Some(ElseBranch::Block(b)) => ccm_walk_block(b, names, types, resolved_types),
                Some(ElseBranch::If(e2)) => ccm_walk_expr(e2, names, types, resolved_types),
                None => {}
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            ccm_walk_expr(scrutinee, names, types, resolved_types);
            for a in arms {
                match &a.body {
                    MatchArmBody::Expr(be) => ccm_walk_expr(be, names, types, resolved_types),
                    MatchArmBody::Block(bb) => ccm_walk_block(bb, names, types, resolved_types),
                }
            }
        }
        ExprKind::For { iter, body, .. } => {
            ccm_walk_expr(iter, names, types, resolved_types);
            ccm_walk_block(body, names, types, resolved_types);
        }
        ExprKind::While { cond, body, .. } => {
            ccm_walk_expr(cond, names, types, resolved_types);
            ccm_walk_block(body, names, types, resolved_types);
        }
        ExprKind::WhileLet { scrutinee, body, .. } => {
            ccm_walk_expr(scrutinee, names, types, resolved_types);
            ccm_walk_block(body, names, types, resolved_types);
        }
        ExprKind::Loop { body, .. } => ccm_walk_block(body, names, types, resolved_types),
        ExprKind::Supervised { body, .. } => ccm_walk_block(body, names, types, resolved_types),
        ExprKind::Spawn(inner) => ccm_walk_expr(inner, names, types, resolved_types),
        ExprKind::Detach(b) | ExprKind::Blocking(b) => ccm_walk_block(b, names, types, resolved_types),
        ExprKind::ParallelFor { body, .. } => ccm_walk_block(body, names, types, resolved_types),
        ExprKind::With { body, .. } => ccm_walk_block(body, names, types, resolved_types),
        ExprKind::Forbid { body, .. } | ExprKind::Realtime { body, .. } => ccm_walk_block(body, names, types, resolved_types),
        ExprKind::Try(inner) | ExprKind::Bang(inner) | ExprKind::RefArg(inner) => {
            ccm_walk_expr(inner, names, types, resolved_types)
        }
        _ => {}
        // Deliberately NOT recursed into: `Call` args (a `let` cannot
        // appear there), `Lambda`/`ClosureLight`/`ClosureFull` bodies (a
        // separate scope, see fn doc). Everything else (`Binary`/`Member`/
        // `Index`/...) cannot syntactically CONTAIN a `Stmt::Let` either —
        // only block-bearing constructs can.
    }
}

/// Collects `name -> &closure literal` for every same-function, top-level-
/// visible `let name = <closure literal>` binding in `fd`'s body — the ONE
/// hop [`resolve_arg_closure`] traces for a bare-`Ident` argument. Flat,
/// first-wins-per-name-is-irrelevant (later binding overwrites — `insert`
/// unconditionally, matches "most recent physical `let` for this name" as
/// the natural reading of a shadowing rebind). Same scope-imprecision
/// trade-off as [`collect_capturable_mut`] — documented there.
fn collect_closure_lets<'e>(fd: &'e FnDecl, out: &mut HashMap<String, &'e Expr>) {
    match &fd.body {
        FnBody::Block(b) => ccl_walk_block(b, out),
        FnBody::Expr(e) => ccl_walk_expr(e, out),
        FnBody::External => {}
    }
}

fn ccl_walk_block<'e>(b: &'e Block, out: &mut HashMap<String, &'e Expr>) {
    for s in &b.stmts {
        ccl_walk_stmt(s, out);
    }
    if let Some(t) = &b.trailing {
        ccl_walk_expr(t, out);
    }
}

fn ccl_walk_stmt<'e>(s: &'e Stmt, out: &mut HashMap<String, &'e Expr>) {
    match s {
        Stmt::Let(d) => {
            if is_closure_literal(&d.value) {
                if let Pattern::Ident { name, .. } = &d.pattern {
                    out.insert(name.clone(), &d.value);
                }
            }
            ccl_walk_expr(&d.value, out);
        }
        Stmt::Expr(e) => ccl_walk_expr(e, out),
        Stmt::Defer { body, .. } => ccl_walk_expr(body, out),
        Stmt::ConsumeScope { init, body, .. } => {
            ccl_walk_expr(init, out);
            ccl_walk_block(body, out);
        }
        _ => {}
    }
}

fn ccl_walk_expr<'e>(e: &'e Expr, out: &mut HashMap<String, &'e Expr>) {
    match &e.kind {
        ExprKind::Block(b) => ccl_walk_block(b, out),
        ExprKind::If { cond, then, else_ } => {
            ccl_walk_expr(cond, out);
            ccl_walk_block(then, out);
            match else_ {
                Some(ElseBranch::Block(b)) => ccl_walk_block(b, out),
                Some(ElseBranch::If(e2)) => ccl_walk_expr(e2, out),
                None => {}
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            ccl_walk_expr(scrutinee, out);
            for a in arms {
                match &a.body {
                    MatchArmBody::Expr(be) => ccl_walk_expr(be, out),
                    MatchArmBody::Block(bb) => ccl_walk_block(bb, out),
                }
            }
        }
        ExprKind::For { iter, body, .. } => {
            ccl_walk_expr(iter, out);
            ccl_walk_block(body, out);
        }
        ExprKind::While { cond, body, .. } => {
            ccl_walk_expr(cond, out);
            ccl_walk_block(body, out);
        }
        ExprKind::Loop { body, .. } => ccl_walk_block(body, out),
        ExprKind::Supervised { body, .. } => ccl_walk_block(body, out),
        ExprKind::Spawn(inner) => ccl_walk_expr(inner, out),
        ExprKind::Detach(b) | ExprKind::Blocking(b) => ccl_walk_block(b, out),
        ExprKind::ParallelFor { body, .. } => ccl_walk_block(body, out),
        _ => {}
    }
}

/// "Captures visible" at the call site: `arg` IS a closure literal
/// directly, or a bare `Ident` resolving to a same-function `let name =
/// <closure literal>` seen anywhere earlier/later in `fd`'s own body
/// (`closure_lets`). Anything else (a parameter, a field access, a value
/// forwarded through 2+ hops) declines — `None` — leaving it to the
/// pre-existing, separate indirect-call machinery if it is ALSO reached
/// directly at a seed point.
fn resolve_arg_closure<'e>(arg: &'e Expr, closure_lets: &HashMap<String, &'e Expr>) -> Option<&'e Expr> {
    if is_closure_literal(arg) {
        return Some(arg);
    }
    if let ExprKind::Ident(name) = &arg.kind {
        return closure_lets.get(name).copied();
    }
    None
}

/// Computes a closure literal's OWN [`FnSafety`], as it would be evaluated
/// AT the call site passing it into a required parameter. `ctx.mut_params`
/// must already be the CAPTURABLE set ([`collect_capturable_mut`]'s
/// output), not the ordinary per-fn `mut_params` Ф.1/Ф.2 use — see this
/// section's module doc for why a captured touch is the PROVEN class, not
/// the softer `unguarded_mut_param` one. `sig` is `None` for a bare
/// closure literal, `Some` for the trailing-closure-full sugar shape
/// (`FnSigBody`) — [`check_param_passing`]'s two call shapes.
fn closure_own_facts(
    closure_body: ClosureRef,
    ctx: &PassBCtx,
    tags: &HashMap<Span, FnSafety>,
    fn_index: &HashMap<Span, &FnDecl>,
) -> FnSafety {
    let mut facts = FnFacts::default();
    match closure_body {
        ClosureRef::Literal(e) => pb_walk_closure_body(e, ctx, Vec::new(), &mut facts),
        ClosureRef::Sig(sb) => pb_walk_fn_sig_body(sb, ctx, &mut Vec::new(), &mut facts),
    }
    // Приёмка Ф.3 — see module doc: ANY touch reached inside a closure
    // being passed to a required parameter is rooted at either `@` (already
    // `is_self == true`) or a name from `ctx.mut_params` (the CAPTURABLE
    // set here, not an ordinary fn's own `mut` param) — both are the
    // PROVEN-escaping class for a value being handed across a boundary by
    // construction of being captured, so every touch is forced into that
    // class uniformly rather than relying on `place_root`'s ordinary
    // self-vs-param distinction (which encodes a DIFFERENT, softer
    // question for a routine function's OWN `mut` parameter).
    for t in facts.touches.iter_mut() {
        t.is_self = true;
    }
    tag_from_facts_and_callees(&facts, tags, fn_index)
}

enum ClosureRef<'e> {
    Literal(&'e Expr),
    Sig(&'e FnSigBody),
}

/// Ф.3 entry point: walk EVERY call site in the WHOLE module (module doc —
/// the risk crystallises at the PASSING site, not necessarily inside a
/// fiber-seed construct at all) checking arguments passed into a required
/// parameter position. Mirrors [`check_seed_points`]'s per-`Item::Fn`/
/// `Item::Test` outer loop and full-tree inner walk shape, adapted: no
/// seed-specific collection step, every `Call` anywhere is a candidate.
pub fn check_param_passing(
    module: &Module,
    resolved_callees: &HashMap<ExprId, Span>,
    resolved_types: &HashMap<ExprId, super::ResolvedType>,
    tags: &HashMap<Span, FnSafety>,
    required: &RequiredParams,
    errors: &mut Vec<crate::diag::Diagnostic>,
) {
    if required.reasons.is_empty() {
        return;
    }
    let fn_index = build_fn_index(module);
    let type_decls = build_type_decls(module);
    let guarded_params = compute_guarded_params(module);

    let mut check_fn_body = |fd: &FnDecl, errors: &mut Vec<crate::diag::Diagnostic>| {
        let receiver_type = fd.receiver.as_ref().map(|r| r.type_name.as_str());
        let (mut_params, param_types) = collect_capturable_mut(fd, resolved_types);
        let mut closure_lets: HashMap<String, &Expr> = HashMap::new();
        collect_closure_lets(fd, &mut closure_lets);
        let ctx = PassBCtx {
            resolved_callees,
            fn_index: &fn_index,
            guarded_params: &guarded_params,
            type_decls: &type_decls,
            receiver_type,
            mut_params: &mut_params,
            param_types: &param_types,
        };
        match &fd.body {
            FnBody::Block(b) => cpp_walk_block(b, &ctx, &closure_lets, tags, required, &fn_index, errors),
            FnBody::Expr(e) => cpp_walk_expr(e, &ctx, &closure_lets, tags, required, &fn_index, errors),
            FnBody::External => {}
        }
    };

    for item in &module.items {
        match item {
            Item::Fn(fd) => check_fn_body(fd, errors),
            Item::Test(td) => {
                // Synthesize a receiverless, paramless `FnDecl`-shaped walk
                // by reusing the block walker directly — a `test` body has
                // no params of its own to capture-check FOR, but its
                // top-level `mut let`s are still capturable by a closure
                // literal defined inside it, same as any fn body.
                let mut mut_params: HashSet<String> = HashSet::new();
                let mut param_types: HashMap<String, TypeRef> = HashMap::new();
                ccm_walk_block(&td.body, &mut mut_params, &mut param_types, resolved_types);
                let mut closure_lets: HashMap<String, &Expr> = HashMap::new();
                ccl_walk_block(&td.body, &mut closure_lets);
                let ctx = PassBCtx {
                    resolved_callees,
                    fn_index: &fn_index,
                    guarded_params: &guarded_params,
                    type_decls: &type_decls,
                    receiver_type: None,
                    mut_params: &mut_params,
                    param_types: &param_types,
                };
                cpp_walk_block(&td.body, &ctx, &closure_lets, tags, required, &fn_index, errors);
            }
            _ => {}
        }
    }
}

fn cpp_walk_block(
    b: &Block,
    ctx: &PassBCtx,
    closure_lets: &HashMap<String, &Expr>,
    tags: &HashMap<Span, FnSafety>,
    required: &RequiredParams,
    fn_index: &HashMap<Span, &FnDecl>,
    errors: &mut Vec<crate::diag::Diagnostic>,
) {
    for s in &b.stmts {
        cpp_walk_stmt(s, ctx, closure_lets, tags, required, fn_index, errors);
    }
    if let Some(t) = &b.trailing {
        cpp_walk_expr(t, ctx, closure_lets, tags, required, fn_index, errors);
    }
}

fn cpp_walk_stmt(
    s: &Stmt,
    ctx: &PassBCtx,
    closure_lets: &HashMap<String, &Expr>,
    tags: &HashMap<Span, FnSafety>,
    required: &RequiredParams,
    fn_index: &HashMap<Span, &FnDecl>,
    errors: &mut Vec<crate::diag::Diagnostic>,
) {
    match s {
        Stmt::Let(d) => cpp_walk_expr(&d.value, ctx, closure_lets, tags, required, fn_index, errors),
        Stmt::Expr(e) => cpp_walk_expr(e, ctx, closure_lets, tags, required, fn_index, errors),
        Stmt::Assign { target, value, .. } => {
            cpp_walk_expr(target, ctx, closure_lets, tags, required, fn_index, errors);
            cpp_walk_expr(value, ctx, closure_lets, tags, required, fn_index, errors);
        }
        Stmt::TupleAssign { lhs, rhs, .. } => {
            for e in lhs {
                cpp_walk_expr(e, ctx, closure_lets, tags, required, fn_index, errors);
            }
            for e in rhs {
                cpp_walk_expr(e, ctx, closure_lets, tags, required, fn_index, errors);
            }
        }
        Stmt::Return { value, .. } => {
            if let Some(v) = value {
                cpp_walk_expr(v, ctx, closure_lets, tags, required, fn_index, errors);
            }
        }
        Stmt::Throw { value, .. } => cpp_walk_expr(value, ctx, closure_lets, tags, required, fn_index, errors),
        Stmt::Defer { body, .. } => cpp_walk_expr(body, ctx, closure_lets, tags, required, fn_index, errors),
        Stmt::ConsumeScope { init, body, .. } => {
            cpp_walk_expr(init, ctx, closure_lets, tags, required, fn_index, errors);
            cpp_walk_block(body, ctx, closure_lets, tags, required, fn_index, errors);
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

fn cpp_walk_expr(
    e: &Expr,
    ctx: &PassBCtx,
    closure_lets: &HashMap<String, &Expr>,
    tags: &HashMap<Span, FnSafety>,
    required: &RequiredParams,
    fn_index: &HashMap<Span, &FnDecl>,
    errors: &mut Vec<crate::diag::Diagnostic>,
) {
    if let ExprKind::Call { func, args, trailing } = &e.kind {
        cpp_check_call(e, args, trailing, ctx, closure_lets, tags, required, fn_index, errors);
        cpp_walk_expr(func, ctx, closure_lets, tags, required, fn_index, errors);
        for a in args {
            cpp_walk_expr(a.expr(), ctx, closure_lets, tags, required, fn_index, errors);
        }
        if let Some(t) = trailing {
            match t {
                Trailing::Block(b) => cpp_walk_block(b, ctx, closure_lets, tags, required, fn_index, errors),
                Trailing::LegacyBlockWithParams(tb) => {
                    cpp_walk_block(&tb.body, ctx, closure_lets, tags, required, fn_index, errors)
                }
                Trailing::Fn(sb) => match &sb.body {
                    FnBody::Block(b) => cpp_walk_block(b, ctx, closure_lets, tags, required, fn_index, errors),
                    FnBody::Expr(ex) => cpp_walk_expr(ex, ctx, closure_lets, tags, required, fn_index, errors),
                    FnBody::External => {}
                },
            }
        }
        return;
    }
    match &e.kind {
        ExprKind::Lambda { body, .. } => cpp_walk_expr(body, ctx, closure_lets, tags, required, fn_index, errors),
        ExprKind::ClosureLight { body, .. } => match body {
            ClosureBody::Expr(ex) => cpp_walk_expr(ex, ctx, closure_lets, tags, required, fn_index, errors),
            ClosureBody::Block(b) => cpp_walk_block(b, ctx, closure_lets, tags, required, fn_index, errors),
        },
        ExprKind::ClosureFull(sb) => match &sb.body {
            FnBody::Block(b) => cpp_walk_block(b, ctx, closure_lets, tags, required, fn_index, errors),
            FnBody::Expr(ex) => cpp_walk_expr(ex, ctx, closure_lets, tags, required, fn_index, errors),
            FnBody::External => {}
        },
        ExprKind::Spawn(inner) => cpp_walk_expr(inner, ctx, closure_lets, tags, required, fn_index, errors),
        ExprKind::Detach(b) | ExprKind::Blocking(b) => {
            cpp_walk_block(b, ctx, closure_lets, tags, required, fn_index, errors)
        }
        ExprKind::ParallelFor { iter, body, .. } => {
            cpp_walk_expr(iter, ctx, closure_lets, tags, required, fn_index, errors);
            cpp_walk_block(body, ctx, closure_lets, tags, required, fn_index, errors);
        }
        ExprKind::Block(b) => cpp_walk_block(b, ctx, closure_lets, tags, required, fn_index, errors),
        ExprKind::If { cond, then, else_ } => {
            cpp_walk_expr(cond, ctx, closure_lets, tags, required, fn_index, errors);
            cpp_walk_block(then, ctx, closure_lets, tags, required, fn_index, errors);
            match else_ {
                Some(ElseBranch::Block(b)) => cpp_walk_block(b, ctx, closure_lets, tags, required, fn_index, errors),
                Some(ElseBranch::If(e2)) => cpp_walk_expr(e2, ctx, closure_lets, tags, required, fn_index, errors),
                None => {}
            }
        }
        ExprKind::IfLet { scrutinee, guard, then, else_, .. } => {
            cpp_walk_expr(scrutinee, ctx, closure_lets, tags, required, fn_index, errors);
            if let Some(g) = guard {
                cpp_walk_expr(g, ctx, closure_lets, tags, required, fn_index, errors);
            }
            cpp_walk_block(then, ctx, closure_lets, tags, required, fn_index, errors);
            match else_ {
                Some(ElseBranch::Block(b)) => cpp_walk_block(b, ctx, closure_lets, tags, required, fn_index, errors),
                Some(ElseBranch::If(e2)) => cpp_walk_expr(e2, ctx, closure_lets, tags, required, fn_index, errors),
                None => {}
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            cpp_walk_expr(scrutinee, ctx, closure_lets, tags, required, fn_index, errors);
            for a in arms {
                if let Some(g) = &a.guard {
                    cpp_walk_expr(g, ctx, closure_lets, tags, required, fn_index, errors);
                }
                match &a.body {
                    MatchArmBody::Expr(be) => cpp_walk_expr(be, ctx, closure_lets, tags, required, fn_index, errors),
                    MatchArmBody::Block(bb) => cpp_walk_block(bb, ctx, closure_lets, tags, required, fn_index, errors),
                }
            }
        }
        ExprKind::For { iter, body, .. } => {
            cpp_walk_expr(iter, ctx, closure_lets, tags, required, fn_index, errors);
            cpp_walk_block(body, ctx, closure_lets, tags, required, fn_index, errors);
        }
        ExprKind::While { cond, body, .. } => {
            cpp_walk_expr(cond, ctx, closure_lets, tags, required, fn_index, errors);
            cpp_walk_block(body, ctx, closure_lets, tags, required, fn_index, errors);
        }
        ExprKind::WhileLet { scrutinee, guard, body, .. } => {
            cpp_walk_expr(scrutinee, ctx, closure_lets, tags, required, fn_index, errors);
            if let Some(g) = guard {
                cpp_walk_expr(g, ctx, closure_lets, tags, required, fn_index, errors);
            }
            cpp_walk_block(body, ctx, closure_lets, tags, required, fn_index, errors);
        }
        ExprKind::Loop { body, .. } => cpp_walk_block(body, ctx, closure_lets, tags, required, fn_index, errors),
        ExprKind::Supervised { body, cancel, deadline, on_timeout } => {
            if let Some(c) = cancel {
                cpp_walk_expr(c, ctx, closure_lets, tags, required, fn_index, errors);
            }
            if let Some(dl) = deadline {
                cpp_walk_expr(&dl.expr, ctx, closure_lets, tags, required, fn_index, errors);
            }
            if let Some(oh) = on_timeout {
                cpp_walk_expr(oh, ctx, closure_lets, tags, required, fn_index, errors);
            }
            cpp_walk_block(body, ctx, closure_lets, tags, required, fn_index, errors);
        }
        ExprKind::With { bindings, body } => {
            for b in bindings {
                cpp_walk_expr(&b.handler, ctx, closure_lets, tags, required, fn_index, errors);
            }
            cpp_walk_block(body, ctx, closure_lets, tags, required, fn_index, errors);
        }
        ExprKind::Forbid { body, .. } | ExprKind::Realtime { body, .. } => {
            cpp_walk_block(body, ctx, closure_lets, tags, required, fn_index, errors)
        }
        ExprKind::Try(inner) | ExprKind::Bang(inner) | ExprKind::RefArg(inner) => {
            cpp_walk_expr(inner, ctx, closure_lets, tags, required, fn_index, errors)
        }
        ExprKind::Coalesce(a, b) => {
            cpp_walk_expr(a, ctx, closure_lets, tags, required, fn_index, errors);
            cpp_walk_expr(b, ctx, closure_lets, tags, required, fn_index, errors);
        }
        ExprKind::As(inner, _) | ExprKind::Is(inner, _) => {
            cpp_walk_expr(inner, ctx, closure_lets, tags, required, fn_index, errors)
        }
        ExprKind::Binary { left, right, .. } => {
            cpp_walk_expr(left, ctx, closure_lets, tags, required, fn_index, errors);
            cpp_walk_expr(right, ctx, closure_lets, tags, required, fn_index, errors);
        }
        ExprKind::Unary { operand, .. } => cpp_walk_expr(operand, ctx, closure_lets, tags, required, fn_index, errors),
        ExprKind::Member { obj, .. } => cpp_walk_expr(obj, ctx, closure_lets, tags, required, fn_index, errors),
        ExprKind::Index { obj, index } => {
            cpp_walk_expr(obj, ctx, closure_lets, tags, required, fn_index, errors);
            cpp_walk_expr(index, ctx, closure_lets, tags, required, fn_index, errors);
        }
        ExprKind::TupleLit(elems) => {
            for el in elems {
                cpp_walk_expr(el, ctx, closure_lets, tags, required, fn_index, errors);
            }
        }
        ExprKind::TurboFish { base, .. } => cpp_walk_expr(base, ctx, closure_lets, tags, required, fn_index, errors),
        _ => {}
    }
}

/// The actual check, at one `Call` node: for every parameter position the
/// resolved target requires (`RequiredParams::is_required`), resolve the
/// argument to a closure ([`resolve_arg_closure`] for a positional
/// argument, or the trailing-closure-full sugar bound to the LAST
/// parameter — Nova's own trailing-closure convention, mirrors `pb_walk_
/// call_args`'s identical `trailing_idx` computation), compute its OWN
/// safety ([`closure_own_facts`]), and emit `E_FIBER_UNSAFE_ARG` if it is
/// not `Safe`.
fn cpp_check_call(
    e: &Expr,
    args: &[CallArg],
    trailing: &Option<Trailing>,
    ctx: &PassBCtx,
    closure_lets: &HashMap<String, &Expr>,
    tags: &HashMap<Span, FnSafety>,
    required: &RequiredParams,
    fn_index: &HashMap<Span, &FnDecl>,
    errors: &mut Vec<crate::diag::Diagnostic>,
) {
    let Some(target) = ctx.resolved_callees.get(&e.id).copied() else { return };
    let Some(target_fd) = fn_index.get(&target).copied() else { return };
    let callee_arity = target_fd.params.len();
    let trailing_idx = if trailing.is_some() { Some(callee_arity.saturating_sub(1)) } else { None };

    for j in 0..callee_arity {
        if !required.is_required(target, j) {
            continue;
        }
        let found: Option<(Span, FnSafety)> = if let Some(a) = args.get(j) {
            resolve_arg_closure(a.expr(), closure_lets)
                .map(|cl| (a.expr().span, closure_own_facts(ClosureRef::Literal(cl), ctx, tags, fn_index)))
        } else if trailing_idx == Some(j) {
            match trailing {
                Some(Trailing::Fn(sb)) => {
                    Some((sb.span, closure_own_facts(ClosureRef::Sig(sb), ctx, tags, fn_index)))
                }
                _ => None,
            }
        } else {
            None
        };
        let Some((arg_span, fs)) = found else { continue };
        if fs.tag == Tag::Safe {
            continue;
        }
        let fname = render_fn_name(target_fd);
        let pname = target_fd.params[j].name.clone();
        let req_chain = required.render_chain((target, j), fn_index);
        let mut msg = format!(
            "[E_FIBER_UNSAFE_ARG] passing this closure to `{fname}`'s parameter \
             `{pname}` is unsafe — that parameter REQUIRES a `Safe` closure \
             (Plan 238 Ф.3, D446 §4/§5 амендмент: the requirement is either \
             inferred from `{fname}`'s own body (or from a callee it forwards \
             `{pname}` to, unchanged), or an explicit `#fiber_safe` annotation \
             — see the chain below — enforced at every call site that passes \
             an argument, not just where the parameter is itself invoked). The \
             closure passed here is `{tag:?}` (cause: {reason}): it captures \
             mutable state reachable from the enclosing scope with no live \
             lock and no `#share`/`consume` proof. Under M:N scheduling this \
             closure may run concurrently with, or migrate across threads \
             from, the code that captured it — an unguarded mutation reached \
             through it is a data race. Requirement chain: {chain}.",
            fname = fname,
            pname = pname,
            tag = fs.tag,
            reason = terminal_reason_text(fs.reason),
            chain = req_chain,
        );
        if let Some(detail_span) = fs.detail_span {
            msg.push_str(&format!(
                " Closure's own callee chain: {}.",
                render_chain(detail_span, tags, fn_index)
            ));
        }
        msg.push_str(
            " Fix: guard the captured state under a live lock before \
             capturing it, make its type `#share`-safe, or pass a closure \
             that only touches exclusively-owned/`#share`-verified state.",
        );
        errors.push(crate::diag::Diagnostic::new(msg, arg_span));
    }
}
