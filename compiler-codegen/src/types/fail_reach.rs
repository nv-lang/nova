// Plan 221.1 п.11 №428 (D62/№113, "энфорс проверяет форму, а не свойство"):
// TRANSITIVE `Fail`-reachability over the LOCAL call graph, replacing the
// old syntax-only `check_bang_requires_fail_block`/`_expr` walker that only
// ever looked at the literal `!!` tokens written directly inside an
// `export fn`'s OWN body. That walker was blind to the ordinary case of
// calling into ANOTHER function (private helper, any depth of chaining,
// or even a fully public callee) that itself needs `Fail` — probe:
// `export fn api(x) -> int => level1(x)` where `level1` calls `level2`
// calls `level3`, and only `level3` has the literal `risky(x)!!` — passed
// `nova check` silently (D62's "явная декларация обязательна" was
// decoration for anything but the direct-body shape).
//
// ── Why a NEW pass instead of extending `check_fn` in place ────────────
// `check_bang_requires_fail_*` used to run from inside `TypeCheckCtx::
// check_fn`, called once per `Item::Fn` as `check_module` walks the
// module — at that point `resolved_callees` (the ExprId → callee-`FnDecl`-
// span channel, §0/196) is only PARTIALLY populated (it fills in AS a
// function's own body is type-checked, and functions are visited in
// declaration order — a caller visited before its callee has an
// incomplete/empty view of that specific edge). Resolving a call needs the
// FULL picture: which local function (if any) a given `Call` expr's callee
// resolves to, INCLUDING instance-method/generic-dispatch resolution that
// only full type-inference can do (a bare name/path lookup, the kind
// `CapabilityCtx::check_capabilities_at` does for its OWN effect checks,
// cannot see through an `obj.method()` call at all — see that fn's own
// doc: "Только receiver-Path формы... requires type-инференции, отложен").
//
// `fiber_safety.rs` (Plan 238) already solved exactly this ordering problem
// for a DIFFERENT graph-shaped property (race-safety tags) the same way:
// run as a SEPARATE, ADDITIVE pass, called once from `check_module_impl`
// right after `type_check_ctx.check_module(...)` returns — at that point
// `resolved_callees` is FINAL for the whole module. `run` (the export-
// boundary entry point, below) mirrors that architecture (see its call
// site in `mod.rs`, right next to `fiber_safety::run`).
//
// `check_defer_bodies` (D158, `mod.rs`) has the SAME transitivity gap for
// `defer`/`errdefer` bodies, but runs EARLIER in the pipeline (BEFORE
// `TypeCheckCtx::check_module`, so `resolved_callees` does not exist yet
// there) — `requires_fail_by_name` (bottom of this file) is a NAME-based
// variant of the same fixed point, usable at that earlier point.
//
// ── The property, precisely ─────────────────────────────────────────────
// `requires_fail(F)` (a LOCAL fn `F`, `Item::Fn` in `module.items`) holds
// iff:
//   (a) `F`'s OWN signature already carries `Fail[…]` (`has_fail_effect`) —
//       covers a callee that is CORRECTLY declared (public API contract:
//       any properly Fail-annotated function propagates the obligation to
//       ITS callers just by being called, `!!`/`?` at the call site is NOT
//       required — see probe3 in the PROGRESS doc: a plain call
//       `declared_fail(x)` with no unwrap operator at all, into a fn that
//       explicitly writes `Fail[str]`, ALSO passed `nova check` silently
//       before this fix — the old walker never looked at a callee's
//       effect row, full stop, regardless of hop count), OR
//   (b) `F`'s own body — walked INLINE through nested closures/handler-
//       literal op bodies (their effect capability is the ENCLOSING scope's,
//       not their own — see `ClosureLight`'s own doc comment, "эффекты —
//       parent fn'а + активных with-блоков") — reaches an un-discharged
//       `!!` (D85: always throw-style, unlike `?` which is dual and stays
//       out of this walker's scope exactly like the OLD one), OR
//   (c) `F`'s body reaches (same inline scope) a `Call` whose RESOLVED
//       target is another LOCAL fn `G` with `requires_fail(G)` — UNLESS
//       that call site sits inside a `with Fail = …` handler installed
//       earlier in the SAME body (D158/D11 — discharged locally, mirrors
//       the old walker's `fail_ok` threading exactly).
//
// Computed as a least fixed point over `fn_index` (bounded — at most
// `fn_index.len()` rounds add anything). Recursive/mutually-recursive call
// graphs terminate safely: a `Call` is resolved by KEY LOOKUP into the
// (partially built) `requires` set, never by inlining the callee's AST —
// so there is no risk of infinite walker recursion on a cycle, only a
// possibly-slower fixed point convergence.
//
// The walker (`walk_expr`/`walk_block` below) is generic over the call-
// resolution KEY type `K` so the ONE AST traversal serves BOTH variants:
// `run`'s export-boundary pass keys by declaration `Span` (via
// `resolved_callees`, fully resolved — methods, generics, everything);
// `requires_fail_by_name`'s early defer-body variant keys by the SAME
// `Type.method`/`name` string `call_target_name` (`mod.rs`) already uses
// (weaker resolution — cannot see through an instance-method call reached
// via a variable — but that is `check_defer_body_inner`'s EXISTING
// resolution ceiling already, not a regression this file introduces).
//
// ── Scope / non-goals (documented, not silently dropped) ───────────────
// - `Item::Fn` from `module.items` ONLY (mirrors `fiber_safety::
//   build_fn_index`'s core scan) — this module.items is already flat
//   (imports/prelude/peer-files merged, `Module.items`'s own doc comment:
//   "остаются flat for backward compat"), so this reaches every fn THIS
//   compile unit can call directly, including imported/prelude ones.
//   `external fn` bodies (`FnBody::External`) cannot carry `Fail` at all
//   (`E_EXTERNAL_FN_FAIL_EFFECT`, D216 §20) — never a source, only ever a
//   dead end for this walk.
// - Cross-module (a different compile unit) callee — the resolved key
//   will not be present in THIS module's `fn_index`; falls through to
//   case (a) only (the callee's OWN already-checked signature is the
//   contract at that boundary — exactly how effect rows compose normally,
//   same "signature is the module boundary contract" stance
//   `fiber_safety.rs` documents for its own `cross_module_callee`).
// - A call whose callee does not resolve at all (dynamic dispatch through
//   a `fn`-typed value, an unresolved generic edge case, …) is invisible
//   to this walk — false-negative direction only, same conservative-but-
//   safe stance as the rest of this file's family of checks;
//   `--strict-effects`' sibling `E_UNDECLARED_TRANSITIVE_EFFECT` has the
//   identical limitation for non-`Fail` effects.
// - Diagnostics only ever fire for `fd.is_export` (D62 scope split
//   unchanged — policy, not touched by this fix). The `requires`/fixed-
//   point SET is computed over every LOCAL fn regardless of export-ness
//   (needed internally to answer "does calling INTO this fn matter"), but
//   nothing is ever emitted for a private fn's own un-declared `Fail` —
//   that stays silent D28 auto-inference, exactly as before.

use std::collections::{HashMap, HashSet};
use std::hash::Hash;

use crate::ast::{
    ArrayElem, Block, ClosureBody, ElseBranch, Expr, ExprId, ExprKind, FnBody, FnDecl,
    HandlerMethodBody, Item, MatchArmBody, Module, SelectOp, Stmt, TypeRef,
};
use crate::diag::{Diagnostic, Span};

/// Stable identity for a local fn/method — its declaration span. Matches
/// `resolved_callees`'s target convention (`fiber_safety::FnId`'s own doc).
type FnId = Span;

fn build_fn_index(module: &Module) -> HashMap<FnId, &FnDecl> {
    let mut idx = HashMap::new();
    for item in &module.items {
        if let Item::Fn(fd) = item {
            idx.insert(fd.span, fd);
        }
    }
    idx
}

fn has_fail_effect(effects: &[TypeRef]) -> bool {
    super::has_fail_effect(effects)
}

/// Sink for the shared walker below — either silently tracks "did we find
/// an un-discharged `Fail` obligation anywhere" (fixed-point pass, no
/// diagnostics), or ALSO pushes a `[E_BANG_REQUIRES_FAIL]` diagnostic at
/// every offending site (export-boundary pass) — mirrors `Diagnostic`
/// collection style used throughout this file family, just parameterized
/// so the two callers share one walker instead of two near-duplicate ones.
struct Sink<'a> {
    errors: Option<&'a mut Vec<Diagnostic>>,
    found: bool,
}

impl<'a> Sink<'a> {
    fn silent() -> Sink<'static> {
        Sink { errors: None, found: false }
    }
    fn collecting(errors: &'a mut Vec<Diagnostic>) -> Sink<'a> {
        Sink { errors: Some(errors), found: false }
    }
    fn hit_bang(&mut self, span: Span) {
        self.found = true;
        if let Some(errs) = self.errors.as_deref_mut() {
            errs.push(Diagnostic::new(
                "[E_BANG_REQUIRES_FAIL] `!!` бросает через эффект `Fail[…]` — добавь `Fail[E]` \
                 в сигнатуру (для `Option` — `Fail[RuntimeNoneError]`), либо обработай значением \
                 (`?? fallback` / `?? panic(\"...\")`/ `match`), либо пробрось `?` (D85, ENFORCED \
                 план 221.1 №113 — `expr!!` всегда throw-стиль, `Fail` в сигнатуре обязателен)."
                    .to_string(),
                span,
            ));
        }
    }
    fn hit_call(&mut self, span: Span, callee_label: &str) {
        self.found = true;
        if let Some(errs) = self.errors.as_deref_mut() {
            errs.push(Diagnostic::new(
                format!(
                    "[E_BANG_REQUIRES_FAIL] call to `{}` requires effect `Fail[…]` (declared \
                     directly on its signature, or reached transitively through ITS OWN calls/\
                     `!!`) — add `Fail[E]` to this `export fn`'s own signature, or install \
                     `with Fail = handler {{ … }}` around this call (D62/№113/№428 ENFORCED: \
                     the export boundary now checks the CALLEE'S resolved effect closure, not \
                     just literal `!!` tokens written directly in this fn's own body — chaining \
                     through private helpers, methods, or generic calls does not exempt an \
                     export fn from declaring `Fail`).",
                    callee_label,
                ),
                span,
            ));
        }
    }
}

/// Generic over the call-resolution KEY `K` — see module doc's "The walker
/// is generic" section. `resolve` maps a `Call` expr to its callee's key
/// (`None` = unresolved, walk continues without a hit); `index`/`requires`
/// are keyed the SAME way.
struct Ctx<'a, K: Eq + Hash + Clone> {
    index: &'a HashMap<K, &'a FnDecl>,
    resolve: &'a dyn Fn(&Expr) -> Option<K>,
    /// While COMPUTING the fixed point, a callee not yet in the set is
    /// simply "not yet known to require Fail" — safe under-approximation
    /// during iteration, corrected by the next round. Final/closed for the
    /// export-boundary and defer-body consumer passes.
    requires: &'a HashSet<K>,
}

fn callee_requires_fail<K: Eq + Hash + Clone>(ctx: &Ctx<K>, key: &K) -> bool {
    ctx.index.get(key).map(|fd| has_fail_effect(&fd.effects)).unwrap_or(false)
        || ctx.requires.contains(key)
}

fn callee_label<K: Eq + Hash + Clone>(ctx: &Ctx<K>, key: &K) -> String {
    ctx.index.get(key).map(|fd| fd.name.clone()).unwrap_or_else(|| "<fn>".to_string())
}

// ── Walker: mirrors the retired `check_bang_requires_fail_block`/`_expr`
// structurally (same AST coverage, same `fail_ok`-threading discipline for
// `with Fail = …`/D158), plus: (1) a `Call` checks its RESOLVED target
// against `requires`/declared-Fail, (2) closures/handler-literal op bodies
// are walked INLINE (their capability comes from the enclosing scope, not
// a scope of their own — see module doc), (3) a `ClosureFull`'s OWN
// `Fail[…]` effect row (if present) discharges ITS OWN body, same as a
// named fn would.

fn walk_block<K: Eq + Hash + Clone>(b: &Block, ctx: &Ctx<K>, fail_ok: bool, sink: &mut Sink) {
    for s in &b.stmts {
        walk_stmt(s, ctx, fail_ok, sink);
    }
    if let Some(t) = &b.trailing {
        walk_expr(t, ctx, fail_ok, sink);
    }
}

fn walk_stmt<K: Eq + Hash + Clone>(s: &Stmt, ctx: &Ctx<K>, fail_ok: bool, sink: &mut Sink) {
    match s {
        Stmt::ConsumeScope { init, body, .. } => {
            walk_expr(init, ctx, fail_ok, sink);
            walk_block(body, ctx, fail_ok, sink);
        }
        // defer/errdefer body — governed by `check_defer_body_inner` (D158),
        // NOT this walker (same split the retired walker documented).
        Stmt::Defer { .. } => {}
        Stmt::Let(decl) => walk_expr(&decl.value, ctx, fail_ok, sink),
        Stmt::Const(_) => {}
        Stmt::Expr(e) => walk_expr(e, ctx, fail_ok, sink),
        Stmt::Assign { target, value, .. } => {
            walk_expr(target, ctx, fail_ok, sink);
            walk_expr(value, ctx, fail_ok, sink);
        }
        Stmt::Return { value, .. } => {
            if let Some(v) = value {
                walk_expr(v, ctx, fail_ok, sink);
            }
        }
        Stmt::Throw { value, .. } => walk_expr(value, ctx, fail_ok, sink),
        Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => {
            walk_expr(expr, ctx, fail_ok, sink)
        }
        Stmt::Apply { args, .. } => {
            for a in args {
                walk_expr(a, ctx, fail_ok, sink);
            }
        }
        Stmt::Calc { steps, .. } => {
            for step in steps {
                walk_expr(&step.expr, ctx, fail_ok, sink);
            }
        }
        Stmt::TupleAssign { lhs, rhs, .. } => {
            for e in lhs {
                walk_expr(e, ctx, fail_ok, sink);
            }
            for e in rhs {
                walk_expr(e, ctx, fail_ok, sink);
            }
        }
        Stmt::Break(_) | Stmt::Continue(_) | Stmt::Reveal { .. } => {}
    }
}

fn walk_expr<K: Eq + Hash + Clone>(e: &Expr, ctx: &Ctx<K>, fail_ok: bool, sink: &mut Sink) {
    match &e.kind {
        ExprKind::Bang(inner) => {
            if !fail_ok {
                sink.hit_bang(e.span);
            }
            walk_expr(inner, ctx, fail_ok, sink);
        }
        ExprKind::Try(inner) | ExprKind::RefArg(inner) | ExprKind::Throw(inner) => {
            walk_expr(inner, ctx, fail_ok, sink);
        }
        ExprKind::Block(b) => walk_block(b, ctx, fail_ok, sink),
        ExprKind::If { cond, then, else_ } => {
            walk_expr(cond, ctx, fail_ok, sink);
            walk_block(then, ctx, fail_ok, sink);
            if let Some(ElseBranch::Block(b)) = else_ {
                walk_block(b, ctx, fail_ok, sink);
            }
            if let Some(ElseBranch::If(e2)) = else_ {
                walk_expr(e2, ctx, fail_ok, sink);
            }
        }
        ExprKind::IfLet { scrutinee, then, else_, .. } => {
            walk_expr(scrutinee, ctx, fail_ok, sink);
            walk_block(then, ctx, fail_ok, sink);
            if let Some(ElseBranch::Block(b)) = else_ {
                walk_block(b, ctx, fail_ok, sink);
            }
            if let Some(ElseBranch::If(e2)) = else_ {
                walk_expr(e2, ctx, fail_ok, sink);
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            walk_expr(scrutinee, ctx, fail_ok, sink);
            for a in arms {
                match &a.body {
                    MatchArmBody::Expr(e2) => walk_expr(e2, ctx, fail_ok, sink),
                    MatchArmBody::Block(b) => walk_block(b, ctx, fail_ok, sink),
                }
                if let Some(g) = &a.guard {
                    walk_expr(g, ctx, fail_ok, sink);
                }
            }
        }
        ExprKind::For { iter, body, .. } | ExprKind::ParallelFor { iter, body, .. } => {
            walk_expr(iter, ctx, fail_ok, sink);
            walk_block(body, ctx, fail_ok, sink);
        }
        ExprKind::While { cond, body, .. } => {
            walk_expr(cond, ctx, fail_ok, sink);
            walk_block(body, ctx, fail_ok, sink);
        }
        ExprKind::WhileLet { scrutinee, body, .. } => {
            walk_expr(scrutinee, ctx, fail_ok, sink);
            walk_block(body, ctx, fail_ok, sink);
        }
        ExprKind::Loop { body, .. } => walk_block(body, ctx, fail_ok, sink),
        ExprKind::Select { arms } => {
            for arm in arms {
                match &arm.op {
                    SelectOp::Recv { chan, .. } => walk_expr(chan, ctx, fail_ok, sink),
                    SelectOp::Send { chan, value } => {
                        walk_expr(chan, ctx, fail_ok, sink);
                        walk_expr(value, ctx, fail_ok, sink);
                    }
                    SelectOp::Default => {}
                }
                if let Some(g) = &arm.guard {
                    walk_expr(g, ctx, fail_ok, sink);
                }
                walk_block(&arm.body, ctx, fail_ok, sink);
            }
        }
        // `with Fail = …` — discharges throws LOCALLY within `body` (D158/
        // D11), same as the retired walker. The handler EXPRESSION(S)
        // themselves are walked too (Plan 221.1 №428, "handler" matrix
        // cell) — a `HandlerLit`'s op bodies run with the OUTER `fail_ok`
        // (installing your own handler does not exempt ITS OWN
        // implementation from declaring how it fails; conservative/safe
        // direction, matches this file family's "not proven ⇒ error"
        // default).
        ExprKind::With { bindings, body } => {
            let has_fail_binding = bindings.iter().any(|wb| {
                matches!(&wb.effect, TypeRef::Named { path, .. } if path.last().map(|s| s.as_str()) == Some("Fail"))
            });
            for wb in bindings {
                walk_handler_expr(&wb.handler, ctx, fail_ok, sink);
            }
            walk_block(body, ctx, fail_ok || has_fail_binding, sink);
        }
        ExprKind::Forbid { body, .. }
        | ExprKind::Realtime { body, .. }
        | ExprKind::Detach(body)
        | ExprKind::Blocking(body) => {
            walk_block(body, ctx, fail_ok, sink);
        }
        ExprKind::Supervised { body, cancel, deadline, on_timeout } => {
            if let Some(c) = cancel {
                walk_expr(c, ctx, fail_ok, sink);
            }
            if let Some(dl) = deadline {
                walk_expr(&dl.expr, ctx, fail_ok, sink);
            }
            if let Some(oh) = on_timeout {
                walk_expr(oh, ctx, fail_ok, sink);
            }
            walk_block(body, ctx, fail_ok, sink);
        }
        ExprKind::Call { func, args, trailing } => {
            if let Some(key) = (ctx.resolve)(e) {
                if callee_requires_fail(ctx, &key) && !fail_ok {
                    sink.hit_call(e.span, &callee_label(ctx, &key));
                }
            }
            walk_expr(func, ctx, fail_ok, sink);
            for a in args {
                walk_expr(a.expr(), ctx, fail_ok, sink);
            }
            if let Some(t) = trailing {
                match t {
                    crate::ast::Trailing::Block(b) => walk_block(b, ctx, fail_ok, sink),
                    crate::ast::Trailing::LegacyBlockWithParams(tb) => {
                        walk_block(&tb.body, ctx, fail_ok, sink)
                    }
                    crate::ast::Trailing::Fn(sb) => walk_closure_sig_body(sb, ctx, fail_ok, sink),
                }
            }
        }
        ExprKind::Spawn(inner) => walk_expr(inner, ctx, fail_ok, sink),
        ExprKind::Binary { left, right, .. } => {
            walk_expr(left, ctx, fail_ok, sink);
            walk_expr(right, ctx, fail_ok, sink);
        }
        ExprKind::Unary { operand, .. } => walk_expr(operand, ctx, fail_ok, sink),
        ExprKind::Coalesce(a, b) => {
            walk_expr(a, ctx, fail_ok, sink);
            walk_expr(b, ctx, fail_ok, sink);
        }
        ExprKind::As(e2, _) | ExprKind::Is(e2, _) => walk_expr(e2, ctx, fail_ok, sink),
        ExprKind::Member { obj, .. } | ExprKind::Index { obj, .. } => {
            walk_expr(obj, ctx, fail_ok, sink)
        }
        ExprKind::TurboFish { base, .. } => walk_expr(base, ctx, fail_ok, sink),
        ExprKind::Interrupt(Some(inner)) => walk_expr(inner, ctx, fail_ok, sink),
        ExprKind::Range { start, end, .. } => {
            if let Some(s) = start {
                walk_expr(s, ctx, fail_ok, sink);
            }
            if let Some(en) = end {
                walk_expr(en, ctx, fail_ok, sink);
            }
        }
        ExprKind::ArrayLit(elems) => {
            for el in elems {
                match el {
                    ArrayElem::Item(e2) | ArrayElem::Spread(e2) => walk_expr(e2, ctx, fail_ok, sink),
                }
            }
        }
        ExprKind::TupleLit(elems) => {
            for el in elems {
                walk_expr(el, ctx, fail_ok, sink);
            }
        }
        ExprKind::RecordLit { fields, .. } => {
            for f in fields {
                if let Some(v) = &f.value {
                    walk_expr(v, ctx, fail_ok, sink);
                }
            }
        }
        // Plan 221.1 №428 (was: skipped — "own scope" waiver): closures
        // carry the ENCLOSING scope's effect capability, not their own
        // (`ClosureLight`'s own AST doc: "эффекты — parent fn'а + активных
        // with-блоков") — walked INLINE with the SAME `fail_ok`.
        ExprKind::Lambda { body, .. } => walk_expr(body, ctx, fail_ok, sink),
        ExprKind::ClosureLight { body, .. } => match body {
            ClosureBody::Expr(e2) => walk_expr(e2, ctx, fail_ok, sink),
            ClosureBody::Block(b) => walk_block(b, ctx, fail_ok, sink),
        },
        ExprKind::ClosureFull(sb) => walk_closure_sig_body(sb, ctx, fail_ok, sink),
        _ => {}
    }
}

fn walk_closure_sig_body<K: Eq + Hash + Clone>(
    sb: &crate::ast::FnSigBody,
    ctx: &Ctx<K>,
    fail_ok: bool,
    sink: &mut Sink,
) {
    // A TYPED closure (`fn(x) Fail[E] -> R { … }`) can declare its OWN
    // effect row — discharges its own body, same as a named fn would.
    let inner_ok = fail_ok || has_fail_effect(&sb.effects);
    match &sb.body {
        FnBody::Block(b) => walk_block(b, ctx, inner_ok, sink),
        FnBody::Expr(e) => walk_expr(e, ctx, inner_ok, sink),
        FnBody::External => {}
    }
}

/// Handler-literal op bodies (`effect X { op(p) => … }` bound in a `with X
/// = handler { … }`), walked inline — see `walk_expr`'s `With` arm doc.
/// A non-`HandlerLit` handler expression (an `Ident` referencing a
/// pre-built handler value, etc.) is walked as an ordinary expr — cannot
/// reach a separate declaration's body this way (documented scope limit,
/// same false-negative-only direction as the rest of this module).
fn walk_handler_expr<K: Eq + Hash + Clone>(handler: &Expr, ctx: &Ctx<K>, fail_ok: bool, sink: &mut Sink) {
    if let ExprKind::HandlerLit { methods, .. } | ExprKind::ProtocolLit { methods, .. } = &handler.kind {
        for m in methods {
            match &m.body {
                HandlerMethodBody::Expr(e2) => walk_expr(e2, ctx, fail_ok, sink),
                HandlerMethodBody::Block(b) => walk_block(b, ctx, fail_ok, sink),
            }
        }
    } else {
        walk_expr(handler, ctx, fail_ok, sink);
    }
}

fn fn_leaks_fail<K: Eq + Hash + Clone>(fd: &FnDecl, ctx: &Ctx<K>) -> bool {
    let mut sink = Sink::silent();
    match &fd.body {
        FnBody::Block(b) => walk_block(b, ctx, false, &mut sink),
        FnBody::Expr(e) => walk_expr(e, ctx, false, &mut sink),
        FnBody::External => {}
    }
    sink.found
}

/// Least fixed point over `index`: `requires_fail(F)` per the module doc's
/// (a)/(b)/(c). Bounded — at most `index.len()` rounds ever add a member
/// (monotone growth, `HashSet` insert-only).
fn compute_requires_fail<K: Eq + Hash + Clone>(
    index: &HashMap<K, &FnDecl>,
    resolve: &dyn Fn(&Expr) -> Option<K>,
) -> HashSet<K> {
    let mut requires: HashSet<K> = HashSet::new();
    // Seed with every fn whose OWN signature already declares `Fail` —
    // case (a), independent of body content (covers `external`/no-body
    // declared-Fail signatures too, though D216 §20 already forbids that
    // combination — harmless to seed unconditionally).
    for (key, fd) in index.iter() {
        if has_fail_effect(&fd.effects) {
            requires.insert(key.clone());
        }
    }
    loop {
        let mut changed = false;
        for (key, fd) in index.iter() {
            if requires.contains(key) {
                continue;
            }
            let ctx = Ctx { index, resolve, requires: &requires };
            if fn_leaks_fail(fd, &ctx) {
                requires.insert(key.clone());
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    requires
}

/// Entry point — called once per `check_module_impl`, right after
/// `resolved_callees` is finalized (same timing as `fiber_safety::run`,
/// see this module's own doc). Emits `[E_BANG_REQUIRES_FAIL]` for every
/// `export fn` whose reachable body (own `!!`/calls, transitively) needs
/// `Fail` and does not declare it / discharge it via a local `with Fail =
/// …`. REPLACES the old in-`check_fn` call to `check_bang_requires_fail_
/// block`/`_expr` (removed — see `mod.rs`'s `check_fn`).
pub fn run(module: &Module, resolved_callees: &HashMap<ExprId, Span>, errors: &mut Vec<Diagnostic>) {
    let index = build_fn_index(module);
    let resolve = |e: &Expr| -> Option<Span> { resolved_callees.get(&e.id).copied() };
    let requires = compute_requires_fail(&index, &resolve);
    for item in &module.items {
        let Item::Fn(fd) = item else { continue };
        if !fd.is_export {
            continue;
        }
        let fail_ok = has_fail_effect(&fd.effects);
        let ctx = Ctx { index: &index, resolve: &resolve, requires: &requires };
        let mut sink = Sink::collecting(errors);
        match &fd.body {
            FnBody::Block(b) => walk_block(b, &ctx, fail_ok, &mut sink),
            FnBody::Expr(e) => walk_expr(e, &ctx, fail_ok, &mut sink),
            FnBody::External => {}
        }
    }
}

/// Same `Type.method`/`name` convention as `check_defer_bodies`' own
/// `fn_effects` map and `call_target_name` (`mod.rs`) — keys MUST line up,
/// `check_defer_body_inner`'s `Call` arm looks BOTH maps up by the SAME
/// `call_target_name(func)` string.
fn fn_name_key(fd: &FnDecl) -> String {
    match &fd.receiver {
        Some(r) => format!("{}.{}", r.type_name, fd.name),
        None => fd.name.clone(),
    }
}

fn build_fn_index_by_name(module: &Module) -> HashMap<String, &FnDecl> {
    let mut idx = HashMap::new();
    for item in &module.items {
        if let Item::Fn(fd) = item {
            // First declaration wins on a name collision — `fn_effects`
            // (mod.rs) MERGES same-name overloads' effects instead
            // (`.or_default().extend(...)`); this index only needs ONE
            // representative body per name to seed the (a)/(b) checks —
            // an overload-effects mismatch is a pre-existing, orthogonal
            // imprecision this NAME-keyed variant inherits from
            // `call_target_name`'s own resolution ceiling (documented in
            // this module's header), not something this fn worsens.
            idx.entry(fn_name_key(fd)).or_insert(fd);
        }
    }
    idx
}

/// NAME-based variant of `compute_requires_fail`/`run` — usable WITHOUT
/// `resolved_callees` (callable EARLY in the pipeline, before
/// `TypeCheckCtx::check_module` populates that channel). Needed by
/// `check_defer_bodies` (`mod.rs`, D158), which runs before that point:
/// its own `fn_effects: HashMap<String, Vec<TypeRef>>` one-hop lookup has
/// the SAME transitivity gap `run` fixes for the export boundary — a
/// `defer` body calling a private helper that only TRANSITIVELY needs
/// `Fail` (chain, not direct) was silently let through by the raw,
/// declaration-only `f.effects` scan `check_defer_bodies` builds
/// `fn_effects` from. Resolution here is NAME-based (`call_target_name`'s
/// own convention) instead of `resolved_callees`-based — the SAME
/// precision ceiling `check_defer_body_inner`'s EXISTING lookup already
/// has (cannot see through an instance-method call reached via a
/// variable), so this closes the TRANSITIVITY gap without changing that
/// pre-existing resolution-precision limit.
pub fn requires_fail_by_name(module: &Module) -> HashSet<String> {
    let index = build_fn_index_by_name(module);
    let resolve = |e: &Expr| -> Option<String> {
        match &e.kind {
            ExprKind::Call { func, .. } => super::call_target_name(func),
            _ => None,
        }
    };
    compute_requires_fail(&index, &resolve)
}
