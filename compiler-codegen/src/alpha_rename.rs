//! Plan 181 (D347) — same-scope re-binding via alpha-renaming.
//!
//! Runs AFTER parse + import-inlining, BEFORE `number_exprs` / type-check /
//! codegen. Nova permits re-declaring a name in the SAME scope with a fresh
//! `ro`/`mut`/`consume` binding whose type may differ (`ro input = read()`;
//! `ro input = parse(input)?`). The front-end already type-checks this with
//! shadowing semantics, but the back-end emits both C declarations under ONE
//! C-name → `redefinition` (§0 of the plan). This pass closes that hole by
//! giving every 2nd-and-later same-scope binding of a name a UNIQUE name
//! (`x__s1`, `x__s2`, …); the original (first) binding keeps its name, so a
//! function with no rebind is a byte-identical no-op (zero-regression).
//!
//! Design (plan §2 — "one alpha-renaming pass"):
//! - Only **same-scope** duplicates are uniquified; nested block shadowing is
//!   left untouched (it already lowers to a nested C block — valid C).
//! - A rebind's RHS is renamed against the PREVIOUS binding (R3: `ro x = x + 1`
//!   reads the old `x`); the new pattern names are declared AFTER.
//! - Closures / `defer` bodies are renamed in program order, so they capture the
//!   binding live at their creation / registration point (R4).
//! - The reserved suffix `__sN` never collides with a user name: a per-function
//!   pre-scan seeds every identifier in the body, and the fresh-name generator
//!   skips any seeded name (plan invariant "`__s\\d+` in user code → уникализировать
//!   глубже").
//!
//! Returned tables:
//! - `shadows`: unique-name → the unique-name it shadows in the same scope. The
//!   consume-checker reads this to fire `E_REBIND_LIVE_CONSUME` (R2) when a
//!   rebind hides an unmet consume obligation.
//! - `originals`: unique-name → original user-written base name (diagnostics /
//!   hover). The pass publishes this map (thread-local, via `set_demangle_map`)
//!   so `demangle_rebind_names` (called by `diag::render`) rewrites a rendered
//!   `x__s1` back to `x` by EXACT set membership — never by a blind `__sN`
//!   pattern that would also mangle a valid user identifier like `buf__s1`. So
//!   `__sN` never reaches user-facing output (acceptance criterion #4).

use crate::ast::*;
use crate::diag::Span;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

/// Side-tables produced by [`alpha_rename`].
#[derive(Debug, Default, Clone)]
pub struct RebindTables {
    /// unique renamed name → the unique name it shadows (same scope, prior binding).
    pub shadows: HashMap<String, String>,
    /// unique renamed name → original user-written base name.
    pub originals: HashMap<String, String>,
    /// [M-consume-rebind-nested-block-shadow] (Plan 172.13): spans of
    /// `consume x = expr` `Stmt::Let`s whose prior binding for `x` is found
    /// in an ENCLOSING (not the current) scope — see [`Renamer::declare_consume`].
    pub consume_reuse_spans: HashSet<Span>,
}

/// Alpha-rename same-scope re-bindings in every function body of `module`.
///
/// Mirrors [`crate::number_exprs`]'s reach (`module.items` + `peer_files`). The
/// renamer is deterministic per top-level item (counters reset on entry), so the
/// `module.items` copy and the `peer_files` copy of the same function are renamed
/// identically — their side-table entries coincide.
pub fn alpha_rename(module: &mut Module) -> RebindTables {
    let mut tables = RebindTables::default();
    for item in &mut module.items {
        rename_item(item, &mut tables);
    }
    for pf in &mut module.peer_files {
        for item in &mut pf.items_here {
            rename_item(item, &mut tables);
        }
    }
    // Publish the shadow map on the module so the consume-checker (R2) can read
    // it without a separate threading channel (§2). `module.items` and the
    // `peer_files` copies rename identically, so their entries coincide.
    module.rebind_shadows = tables.shadows.clone();
    module.consume_reuse_spans = tables.consume_reuse_spans.clone();
    // Publish the synthesized-name → original-name map for `demangle_rebind_names`
    // so diagnostics strip ONLY names this pass actually minted — never a
    // look-alike user identifier such as `buf__s1` (§0). Replaces any prior
    // module's map (one compilation per thread).
    set_demangle_map(&tables.originals);
    tables
}

fn rename_item(item: &mut Item, tables: &mut RebindTables) {
    match item {
        Item::Fn(f) => {
            // Pre-scan every identifier in the signature+body so generated
            // `__sN` names never collide with a user name (plan §2 invariant).
            let mut reserved = HashSet::new();
            for p in &f.params {
                reserved.insert(p.name.clone());
            }
            match &f.body {
                FnBody::Expr(e) => collect_names_expr(e, &mut reserved),
                FnBody::Block(b) => collect_names_block(b, &mut reserved),
                FnBody::External => {}
            }
            let mut r = Renamer::new(tables, reserved);
            r.push_scope();
            // Parameters are first-decls in the function scope; a body
            // `ro x = …` over a param `x` is therefore a same-scope rebind (R6).
            for p in &f.params {
                r.declare_raw(&p.name);
            }
            match &mut f.body {
                FnBody::Expr(e) => r.expr(e),
                // The function body block shares the parameter scope (no extra push).
                FnBody::Block(b) => r.block_in_current_scope(b),
                FnBody::External => {}
            }
            r.pop_scope();
        }
        Item::Test(t) => {
            let mut reserved = HashSet::new();
            collect_names_block(&t.body, &mut reserved);
            let mut r = Renamer::new(tables, reserved);
            r.push_scope();
            r.block_in_current_scope(&mut t.body);
            r.pop_scope();
        }
        Item::Bench(b) => {
            let mut reserved = HashSet::new();
            for s in &b.setup {
                collect_names_stmt(s, &mut reserved);
            }
            collect_names_block(&b.measure_body, &mut reserved);
            for s in &b.teardown {
                collect_names_stmt(s, &mut reserved);
            }
            let mut r = Renamer::new(tables, reserved);
            r.push_scope();
            for s in &mut b.setup {
                r.stmt(s);
            }
            r.block_in_current_scope(&mut b.measure_body);
            for s in &mut b.teardown {
                r.stmt(s);
            }
            r.pop_scope();
        }
        // Module-level const/let bindings are globals (never rebindable), but
        // their initializer may embed a closure whose body has a same-scope
        // rebind — walk the value in a fresh empty scope so nested closures are
        // renamed without exposing the global name.
        Item::Const(c) => {
            let mut reserved = HashSet::new();
            collect_names_expr(&c.value, &mut reserved);
            let mut r = Renamer::new(tables, reserved);
            r.push_scope();
            r.expr(&mut c.value);
            r.pop_scope();
        }
        Item::Let(l) => {
            let mut reserved = HashSet::new();
            collect_names_expr(&l.value, &mut reserved);
            let mut r = Renamer::new(tables, reserved);
            r.push_scope();
            r.expr(&mut l.value);
            r.pop_scope();
        }
        // No expression bodies with locals (mirror number_exprs / desugar).
        Item::Type(_) => {}
        Item::Lemma(_) => {}
    }
}

/// One lexical scope.
#[derive(Default)]
struct Scope {
    /// original base name → current active unique name for declarations made in
    /// THIS scope (inherited outer bindings are found by searching enclosing
    /// scopes).
    map: HashMap<String, String>,
    /// Base names first bound by a *matching-context* pattern (if-let /
    /// while-let / match / for / select-recv). Rebinding such a name in the same
    /// scope is NOT uniquified: that narrow form (`if Some(u) = e { ro u = … }`)
    /// hits a PRE-EXISTING checker stack-overflow (present on baseline d97c0dbe),
    /// so we leave it to lower cleanly to the legacy `redefinition` CC-error
    /// instead of a fresh codegen panic (zero-regression on failure mode).
    /// See [M-181-pattern-var-rebind] (docs/simplifications.md). Plain-`let`
    /// destructure patterns are NOT marked (their rebind DOES uniquify — plan §5).
    pattern_origin: std::collections::HashSet<String>,
}

struct Renamer<'t> {
    scopes: Vec<Scope>,
    /// Per-base-name rebind counter (reset per top-level item).
    counters: HashMap<String, u32>,
    /// Every name that must not be produced as a fresh `__sN` name: all user
    /// identifiers of the current item plus already-generated names.
    reserved: HashSet<String>,
    tables: &'t mut RebindTables,
}

impl<'t> Renamer<'t> {
    fn new(tables: &'t mut RebindTables, reserved: HashSet<String>) -> Self {
        Renamer {
            scopes: Vec::new(),
            counters: HashMap::new(),
            reserved,
            tables,
        }
    }

    fn push_scope(&mut self) {
        self.scopes.push(Scope::default());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    /// Resolve a name reference to the unique name of the binding currently in
    /// scope. Free / global names (not found in any scope) are returned as-is.
    fn resolve(&self, name: &str) -> Option<String> {
        for s in self.scopes.iter().rev() {
            if let Some(u) = s.map.get(name) {
                return Some(u.clone());
            }
        }
        None
    }

    /// Declare `name` in the current (innermost) scope. If the SAME scope
    /// already binds `name`, this is a same-scope rebind: mint a fresh unique
    /// name, record the shadow / original tables, and return the fresh name.
    /// Otherwise `name` keeps its identity (first decl or nested shadow).
    ///
    /// Exception: a rebind whose prior binding is a matching-context pattern
    /// binding (see [`Scope::pattern_origin`]) is left un-uniquified — the
    /// downstream checker/codegen for that narrow form is pre-existing-broken.
    fn declare(&mut self, name: &str, from_pattern: bool) -> String {
        if name == "_" {
            return "_".to_string();
        }
        let scope = self.scopes.last().expect("declare requires an active scope");
        let existing = scope.map.get(name).cloned();
        let is_first = existing.is_none();
        let over_pattern = scope.pattern_origin.contains(name);
        let unique = match existing {
            Some(_) if over_pattern => {
                // Rebind over a matching-context pattern binding — do NOT rename
                // (leave the pre-existing legacy behaviour: `redefinition` CC-error).
                name.to_string()
            }
            Some(prev) => {
                let fresh = self.fresh(name);
                self.tables.shadows.insert(fresh.clone(), prev);
                self.tables.originals.insert(fresh.clone(), name.to_string());
                fresh
            }
            None => name.to_string(),
        };
        let scope = self.scopes.last_mut().expect("declare requires an active scope");
        scope.map.insert(name.to_string(), unique.clone());
        if from_pattern && is_first {
            scope.pattern_origin.insert(name.to_string());
        }
        unique
    }

    /// Declare without needing the returned name (parameters, const names).
    fn declare_raw(&mut self, name: &str) {
        let _ = self.declare(name, false);
    }

    /// [M-consume-rebind-nested-block-shadow] (Plan 172.13): declare a
    /// `consume x = expr` simple-ident binding. Three cases:
    /// - `name` unbound anywhere in scope → fresh first declaration
    ///   (delegates to [`Self::declare`], which takes the `None` branch).
    /// - `name` already bound in the CURRENT (innermost) scope → an ordinary
    ///   same-scope rebind; delegates to [`Self::declare`]'s existing
    ///   fresh-unique-name mechanism (unchanged behaviour).
    /// - `name` bound in an ENCLOSING (not current) scope only → the prior
    ///   binding has a live C declaration in a scope that STRICTLY encloses
    ///   this one (an ancestor block, or the function scope itself). Nova
    ///   treats a `consume`-rebind of it as updating that SAME logical
    ///   variable (D347/D9 semantics), not a fresh block-scoped shadow — so
    ///   leave the name untouched (no rename, no new scope-map entry; reads
    ///   after this point keep resolving to the enclosing scope's existing
    ///   mapping via `resolve()`) and record the statement's span so codegen
    ///   emits a plain reassignment reusing the existing C variable instead
    ///   of a new block-scoped declaration (which would silently go out of
    ///   scope, leaving the outer variable stale/already-consumed).
    fn declare_consume(&mut self, name: &str, span: Span) -> String {
        if name == "_" {
            return "_".to_string();
        }
        let in_current = self
            .scopes
            .last()
            .map(|s| s.map.contains_key(name))
            .unwrap_or(false);
        if !in_current && self.resolve(name).is_some() {
            self.tables.consume_reuse_spans.insert(span);
            name.to_string()
        } else {
            self.declare(name, false)
        }
    }

    /// Mint a fresh `name__sN` that collides with no reserved/user name.
    fn fresh(&mut self, base: &str) -> String {
        loop {
            let n = self.counters.entry(base.to_string()).or_insert(0);
            *n += 1;
            let candidate = format!("{}__s{}", base, n);
            if !self.reserved.contains(&candidate) {
                self.reserved.insert(candidate.clone());
                return candidate;
            }
            // candidate collides with a user identifier — bump and retry.
        }
    }

    // ── statements ────────────────────────────────────────────────────────

    /// Walk a block's statements + trailing in the CURRENT scope (used for
    /// function/test/bench bodies that share the parameter scope).
    fn block_in_current_scope(&mut self, b: &mut Block) {
        for s in &mut b.stmts {
            self.stmt(s);
        }
        if let Some(t) = &mut b.trailing {
            self.expr(t);
        }
    }

    /// Walk a block in a fresh nested scope.
    fn block(&mut self, b: &mut Block) {
        self.push_scope();
        self.block_in_current_scope(b);
        self.pop_scope();
    }

    fn stmt(&mut self, s: &mut Stmt) {
        match s {
            Stmt::Let(d) => {
                // R3: the RHS sees the PREVIOUS binding — rename it first.
                self.expr(&mut d.value);
                // [M-consume-rebind-nested-block-shadow]: a `consume x = expr`
                // simple-ident rebind gets the enclosing-scope-aware path
                // (see `declare_consume`); every other pattern shape keeps the
                // generic same-scope-only `declare_pattern`.
                if d.consume {
                    if let Pattern::Ident { name, .. } = &mut d.pattern {
                        *name = self.declare_consume(&name.clone(), d.span);
                        return;
                    }
                }
                self.declare_pattern(&mut d.pattern, false);
            }
            Stmt::Const(d) => {
                self.expr(&mut d.value);
                self.declare_raw(&d.name.clone());
            }
            Stmt::Expr(e) => self.expr(e),
            Stmt::Assign { target, value, .. } => {
                self.expr(target);
                self.expr(value);
            }
            Stmt::TupleAssign { lhs, rhs, .. } => {
                for e in lhs {
                    self.expr(e);
                }
                for e in rhs {
                    self.expr(e);
                }
            }
            Stmt::Return { value, .. } => {
                if let Some(v) = value {
                    self.expr(v);
                }
            }
            Stmt::Throw { value, .. } => self.expr(value),
            Stmt::Break(_) | Stmt::Continue(_) => {}
            Stmt::Defer { body, .. } => self.expr(body),
            Stmt::ConsumeScope { binding, init, body, result, .. } => {
                // init evaluated in the enclosing scope (R3 order).
                self.expr(init);
                // `binding` is scoped to `body` (D188) → fresh child scope.
                self.push_scope();
                *binding = self.declare(&binding.clone(), false);
                self.block_in_current_scope(body);
                self.pop_scope();
                // Plan 201: result-приёмник блока-выражения — объявляется
                // в ОБЪЕМЛЮЩЕМ scope ПОСЛЕ блока (как обычный let).
                if let Some(r) = result {
                    r.name = self.declare(&r.name.clone(), false);
                }
            }
            Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => self.expr(expr),
            Stmt::Apply { args, .. } => {
                for a in args {
                    self.expr(a);
                }
            }
            Stmt::Calc { steps, .. } => {
                for st in steps {
                    self.expr(&mut st.expr);
                }
            }
            Stmt::Reveal { .. } => {}
        }
    }

    // ── patterns (declarations) ───────────────────────────────────────────

    /// Declare (and rename in-place) every binding introduced by `pat`.
    /// `from_pattern` = true for matching-context patterns (if-let / while-let /
    /// match / for / select), false for plain-`let` destructures.
    fn declare_pattern(&mut self, pat: &mut Pattern, from_pattern: bool) {
        match pat {
            Pattern::Ident { name, .. } => {
                *name = self.declare(&name.clone(), from_pattern);
            }
            Pattern::Binding { name, inner, .. } => {
                *name = self.declare(&name.clone(), from_pattern);
                self.declare_pattern(inner, from_pattern);
            }
            Pattern::Tuple(ps, _) => {
                for p in ps {
                    self.declare_pattern(p, from_pattern);
                }
            }
            Pattern::Array { elems, .. } => {
                for el in elems {
                    match el {
                        ArrayPatternElem::Item(p) => self.declare_pattern(p, from_pattern),
                        ArrayPatternElem::RestBind(name) => {
                            *name = self.declare(&name.clone(), from_pattern);
                        }
                        ArrayPatternElem::Rest => {}
                    }
                }
            }
            Pattern::Record { fields, .. } => {
                for f in fields {
                    match &mut f.pattern {
                        Some(p) => self.declare_pattern(p, from_pattern),
                        // shorthand `{ name }` binds `name`.
                        None => {
                            f.name = self.declare(&f.name.clone(), from_pattern);
                        }
                    }
                }
            }
            Pattern::Variant { kind, .. } => match kind {
                VariantPatternKind::Tuple { patterns, .. } => {
                    for p in patterns {
                        self.declare_pattern(p, from_pattern);
                    }
                }
                VariantPatternKind::Unit => {}
            },
            Pattern::Or { alternatives, .. } => {
                // Alternation binds the same names in each arm; declare from the
                // first (bootstrap semantics — types/mod.rs uses the first arm).
                if let Some(first) = alternatives.first_mut() {
                    self.declare_pattern(first, from_pattern);
                }
            }
            Pattern::Wildcard(_) | Pattern::Literal(_, _) => {}
        }
    }

    /// Declare the bindings of a *matching-context* pattern (if-let / while-let /
    /// match / for / select). Marks them as pattern-origin so a same-scope
    /// rebind of one is left un-uniquified (see [`Scope::pattern_origin`]).
    fn bind_pattern(&mut self, pat: &mut Pattern) {
        self.declare_pattern(pat, true);
    }

    // ── expressions ───────────────────────────────────────────────────────

    fn expr(&mut self, e: &mut Expr) {
        match &mut e.kind {
            ExprKind::Ident(name) => {
                if let Some(u) = self.resolve(name) {
                    *name = u;
                }
            }
            // `Module.name` / `Type.method` — the head is a type/module, never a
            // local; leave untouched.
            ExprKind::Path(_) => {}
            ExprKind::SelfAccess => {}
            ExprKind::IntLit(_)
            | ExprKind::FloatLit(_)
            | ExprKind::BoolLit(_)
            | ExprKind::StrLit(_)
            | ExprKind::CharLit(_)
            | ExprKind::UnitLit
            | ExprKind::HexBlobLit(_) | ExprKind::NullPtrLit => {}

            ExprKind::InterpolatedStr { parts } => {
                for p in parts {
                    if let InterpStrPart::Expr { expr, .. } = p {
                        self.expr(expr);
                    }
                }
            }
            ExprKind::ArrayLit(elems) => {
                for el in elems {
                    match el {
                        ArrayElem::Item(x) | ArrayElem::Spread(x) => self.expr(x),
                    }
                }
            }
            ExprKind::MapLit { elems, .. } => {
                for me in elems {
                    match me {
                        MapElem::Pair(k, v) => {
                            self.expr(k);
                            self.expr(v);
                        }
                        MapElem::Spread(x) => self.expr(x),
                    }
                }
            }
            ExprKind::RecordLit { fields, .. } => {
                for f in fields {
                    if let Some(v) = &mut f.value {
                        self.expr(v);
                    }
                }
            }
            ExprKind::TupleLit(elems) => {
                for x in elems {
                    self.expr(x);
                }
            }
            ExprKind::Member { obj, .. } => self.expr(obj),
            ExprKind::Index { obj, index } => {
                self.expr(obj);
                self.expr(index);
            }
            ExprKind::TurboFish { base, .. } => self.expr(base),
            ExprKind::Call { func, args, trailing } => {
                self.expr(func);
                for a in args {
                    match a {
                        CallArg::Item(x) | CallArg::Spread(x) => self.expr(x),
                        CallArg::Named { value, .. } => self.expr(value),
                    }
                }
                if let Some(t) = trailing {
                    self.trailing(t);
                }
            }
            ExprKind::Try(x) | ExprKind::Bang(x) | ExprKind::RefArg(x) => self.expr(x),
            ExprKind::Coalesce(a, b) => {
                self.expr(a);
                self.expr(b);
            }
            ExprKind::As(x, _) | ExprKind::Is(x, _) => self.expr(x),
            ExprKind::Binary { left, right, .. } => {
                self.expr(left);
                self.expr(right);
            }
            ExprKind::Unary { operand, .. } => self.expr(operand),

            ExprKind::If { cond, then, else_ } => {
                self.expr(cond);
                self.block(then);
                if let Some(eb) = else_ {
                    self.else_branch(eb);
                }
            }
            ExprKind::IfLet { pattern, scrutinee, guard, then, else_ } => {
                // scrutinee evaluated in the enclosing scope.
                self.expr(scrutinee);
                self.push_scope();
                self.bind_pattern(pattern);
                if let Some(g) = guard {
                    self.expr(g);
                }
                self.block_in_current_scope(then);
                self.pop_scope();
                if let Some(eb) = else_ {
                    self.else_branch(eb);
                }
            }
            ExprKind::Match { scrutinee, arms } => {
                self.expr(scrutinee);
                for arm in arms {
                    self.push_scope();
                    self.bind_pattern(&mut arm.pattern);
                    if let Some(g) = &mut arm.guard {
                        self.expr(g);
                    }
                    match &mut arm.body {
                        MatchArmBody::Expr(x) => self.expr(x),
                        MatchArmBody::Block(b) => self.block_in_current_scope(b),
                    }
                    self.pop_scope();
                }
            }
            ExprKind::For { pattern, iter, body, invariants, decreases, .. } => {
                self.expr(iter);
                self.push_scope();
                self.bind_pattern(pattern);
                for inv in invariants {
                    self.expr(inv);
                }
                if let Some(d) = decreases {
                    self.expr(d);
                }
                self.block_in_current_scope(body);
                self.pop_scope();
            }
            ExprKind::ParallelFor { pattern, iter, body, .. } => {
                self.expr(iter);
                self.push_scope();
                self.bind_pattern(pattern);
                self.block_in_current_scope(body);
                self.pop_scope();
            }
            ExprKind::While { cond, body, invariants, decreases } => {
                self.expr(cond);
                for inv in invariants {
                    self.expr(inv);
                }
                if let Some(d) = decreases {
                    self.expr(d);
                }
                self.block(body);
            }
            ExprKind::WhileLet { pattern, scrutinee, guard, body, invariants, decreases } => {
                self.expr(scrutinee);
                self.push_scope();
                self.bind_pattern(pattern);
                if let Some(g) = guard {
                    self.expr(g);
                }
                for inv in invariants {
                    self.expr(inv);
                }
                if let Some(d) = decreases {
                    self.expr(d);
                }
                self.block_in_current_scope(body);
                self.pop_scope();
            }
            ExprKind::Loop { body, invariants, decreases } => {
                for inv in invariants {
                    self.expr(inv);
                }
                if let Some(d) = decreases {
                    self.expr(d);
                }
                self.block(body);
            }
            ExprKind::Block(b) => self.block(b),
            ExprKind::Spawn(x) => self.expr(x),
            ExprKind::Detach(b) | ExprKind::Blocking(b) => self.block(b),
            ExprKind::Supervised { body, cancel, deadline } => {
                if let Some(c) = cancel {
                    self.expr(c);
                }
                if let Some(_dl) = deadline {
                    let _dl_e = &mut _dl.expr;
                    self.expr(_dl_e);
                }
                self.block(body);
            }
            ExprKind::Forbid { body, .. } | ExprKind::Realtime { body, .. } => self.block(body),
            ExprKind::Throw(x) => self.expr(x),
            // [E_COALESCE_RETURN_FALLBACK]: `X ?? return R` — checker always
            // rejects it before this pass runs; walk the optional payload
            // defensively (same shape as `Interrupt`).
            ExprKind::CoalesceReturnFallback(opt) => {
                if let Some(x) = opt {
                    self.expr(x);
                }
            }
            ExprKind::Interrupt(opt) => {
                if let Some(x) = opt {
                    self.expr(x);
                }
            }
            ExprKind::Range { start, end, .. } => {
                if let Some(s) = start {
                    self.expr(s);
                }
                if let Some(en) = end {
                    self.expr(en);
                }
            }
            ExprKind::TaggedTemplate { tag, args, .. } => {
                self.expr(tag);
                for x in args {
                    self.expr(x);
                }
            }

            // ── closures / anonymous functions: fresh param scope ──────────
            ExprKind::Lambda { params, body, .. } => {
                self.push_scope();
                for p in params.iter() {
                    self.declare_raw(&p.name);
                }
                self.expr(body);
                self.pop_scope();
            }
            ExprKind::ClosureLight { params, body } => {
                self.push_scope();
                for p in params.iter() {
                    self.declare_raw(&p.name);
                }
                match body {
                    ClosureBody::Expr(x) => self.expr(x),
                    ClosureBody::Block(b) => self.block_in_current_scope(b),
                }
                self.pop_scope();
            }
            ExprKind::ClosureFull(sb) => {
                self.push_scope();
                for p in sb.params.iter() {
                    self.declare_raw(&p.name);
                }
                match &mut sb.body {
                    FnBody::Expr(x) => self.expr(x),
                    FnBody::Block(b) => self.block_in_current_scope(b),
                    FnBody::External => {}
                }
                self.pop_scope();
            }
            ExprKind::With { bindings, body } => {
                for b in bindings.iter_mut() {
                    self.expr(&mut b.handler);
                }
                self.block(body);
            }
            ExprKind::HandlerLit { methods, .. } | ExprKind::ProtocolLit { methods, .. } => {
                for m in methods.iter_mut() {
                    self.push_scope();
                    for p in m.params.iter() {
                        self.declare_raw(&p.name);
                    }
                    match &mut m.body {
                        HandlerMethodBody::Expr(x) => self.expr(x),
                        HandlerMethodBody::Block(b) => self.block_in_current_scope(b),
                    }
                    self.pop_scope();
                }
            }
            ExprKind::Select { arms } => {
                for arm in arms.iter_mut() {
                    // channel operands evaluated in the enclosing scope.
                    match &mut arm.op {
                        SelectOp::Recv { chan, .. } => self.expr(chan),
                        SelectOp::Send { chan, value } => {
                            self.expr(chan);
                            self.expr(value);
                        }
                        SelectOp::Default => {}
                    }
                    self.push_scope();
                    if let SelectOp::Recv { binding: Some(b), .. } = &mut arm.op {
                        *b = self.declare(&b.clone(), false);
                    }
                    if let Some(g) = &mut arm.guard {
                        self.expr(g);
                    }
                    self.block_in_current_scope(&mut arm.body);
                    self.pop_scope();
                }
            }
            ExprKind::Forall { var, range, body } | ExprKind::Exists { var, range, body } => {
                self.expr(range);
                self.push_scope();
                *var = self.declare(&var.clone(), false);
                self.expr(body);
                self.pop_scope();
            }
        }
    }

    fn else_branch(&mut self, eb: &mut ElseBranch) {
        match eb {
            ElseBranch::Block(b) => self.block(b),
            ElseBranch::If(x) => self.expr(x),
        }
    }

    fn trailing(&mut self, t: &mut Trailing) {
        match t {
            Trailing::Block(b) => self.block(b),
            Trailing::LegacyBlockWithParams(tb) => {
                self.push_scope();
                for p in tb.params.iter() {
                    self.declare_raw(&p.name);
                }
                self.block_in_current_scope(&mut tb.body);
                self.pop_scope();
            }
            Trailing::Fn(sb) => {
                self.push_scope();
                for p in sb.params.iter() {
                    self.declare_raw(&p.name);
                }
                match &mut sb.body {
                    FnBody::Expr(x) => self.expr(x),
                    FnBody::Block(b) => self.block_in_current_scope(b),
                    FnBody::External => {}
                }
                self.pop_scope();
            }
        }
    }
}

// ── pre-scan: collect every identifier in an item (reserve names) ──────────
//
// Only names matter (to avoid `__sN` collisions); we over-approximate by
// collecting all `Ident`/pattern names — a superset is harmless (it only makes
// the fresh generator skip more candidates).

pub(crate) fn collect_names_block(b: &Block, out: &mut HashSet<String>) {
    for s in &b.stmts {
        collect_names_stmt(s, out);
    }
    if let Some(t) = &b.trailing {
        collect_names_expr(t, out);
    }
}

fn collect_names_stmt(s: &Stmt, out: &mut HashSet<String>) {
    match s {
        Stmt::Let(d) => {
            collect_names_pattern(&d.pattern, out);
            collect_names_expr(&d.value, out);
        }
        Stmt::Const(d) => {
            out.insert(d.name.clone());
            collect_names_expr(&d.value, out);
        }
        Stmt::Expr(e) => collect_names_expr(e, out),
        Stmt::Assign { target, value, .. } => {
            collect_names_expr(target, out);
            collect_names_expr(value, out);
        }
        Stmt::TupleAssign { lhs, rhs, .. } => {
            for e in lhs {
                collect_names_expr(e, out);
            }
            for e in rhs {
                collect_names_expr(e, out);
            }
        }
        Stmt::Return { value, .. } => {
            if let Some(v) = value {
                collect_names_expr(v, out);
            }
        }
        Stmt::Throw { value, .. } => collect_names_expr(value, out),
        Stmt::Break(_) | Stmt::Continue(_) | Stmt::Reveal { .. } => {}
        Stmt::Defer { body, .. } => collect_names_expr(body, out),
        Stmt::ConsumeScope { binding, init, body, result, .. } => {
            out.insert(binding.clone());
            if let Some(r) = result {
                out.insert(r.name.clone());
            }
            collect_names_expr(init, out);
            collect_names_block(body, out);
        }
        Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => {
            collect_names_expr(expr, out)
        }
        Stmt::Apply { args, .. } => {
            for a in args {
                collect_names_expr(a, out);
            }
        }
        Stmt::Calc { steps, .. } => {
            for st in steps {
                collect_names_expr(&st.expr, out);
            }
        }
    }
}

fn collect_names_pattern(pat: &Pattern, out: &mut HashSet<String>) {
    match pat {
        Pattern::Ident { name, .. } => {
            out.insert(name.clone());
        }
        Pattern::Binding { name, inner, .. } => {
            out.insert(name.clone());
            collect_names_pattern(inner, out);
        }
        Pattern::Tuple(ps, _) => {
            for p in ps {
                collect_names_pattern(p, out);
            }
        }
        Pattern::Array { elems, .. } => {
            for el in elems {
                match el {
                    ArrayPatternElem::Item(p) => collect_names_pattern(p, out),
                    ArrayPatternElem::RestBind(name) => {
                        out.insert(name.clone());
                    }
                    ArrayPatternElem::Rest => {}
                }
            }
        }
        Pattern::Record { fields, .. } => {
            for f in fields {
                match &f.pattern {
                    Some(p) => collect_names_pattern(p, out),
                    None => {
                        out.insert(f.name.clone());
                    }
                }
            }
        }
        Pattern::Variant { kind, .. } => {
            if let VariantPatternKind::Tuple { patterns, .. } = kind {
                for p in patterns {
                    collect_names_pattern(p, out);
                }
            }
        }
        Pattern::Or { alternatives, .. } => {
            for p in alternatives {
                collect_names_pattern(p, out);
            }
        }
        Pattern::Wildcard(_) | Pattern::Literal(_, _) => {}
    }
}

pub(crate) fn collect_names_expr(e: &Expr, out: &mut HashSet<String>) {
    match &e.kind {
        ExprKind::Ident(name) => {
            out.insert(name.clone());
        }
        ExprKind::Path(_)
        | ExprKind::SelfAccess
        | ExprKind::IntLit(_)
        | ExprKind::FloatLit(_)
        | ExprKind::BoolLit(_)
        | ExprKind::StrLit(_)
        | ExprKind::CharLit(_)
        | ExprKind::UnitLit
        | ExprKind::HexBlobLit(_) | ExprKind::NullPtrLit => {}
        ExprKind::InterpolatedStr { parts } => {
            for p in parts {
                if let InterpStrPart::Expr { expr, .. } = p {
                    collect_names_expr(expr, out);
                }
            }
        }
        ExprKind::ArrayLit(elems) => {
            for el in elems {
                match el {
                    ArrayElem::Item(x) | ArrayElem::Spread(x) => collect_names_expr(x, out),
                }
            }
        }
        ExprKind::MapLit { elems, .. } => {
            for me in elems {
                match me {
                    MapElem::Pair(k, v) => {
                        collect_names_expr(k, out);
                        collect_names_expr(v, out);
                    }
                    MapElem::Spread(x) => collect_names_expr(x, out),
                }
            }
        }
        ExprKind::RecordLit { fields, .. } => {
            for f in fields {
                if let Some(v) = &f.value {
                    collect_names_expr(v, out);
                }
            }
        }
        ExprKind::TupleLit(elems) => {
            for x in elems {
                collect_names_expr(x, out);
            }
        }
        ExprKind::Member { obj, .. } => collect_names_expr(obj, out),
        ExprKind::Index { obj, index } => {
            collect_names_expr(obj, out);
            collect_names_expr(index, out);
        }
        ExprKind::TurboFish { base, .. } => collect_names_expr(base, out),
        ExprKind::Call { func, args, trailing } => {
            collect_names_expr(func, out);
            for a in args {
                collect_names_expr(a.expr(), out);
            }
            if let Some(t) = trailing {
                collect_names_trailing(t, out);
            }
        }
        ExprKind::Try(x) | ExprKind::Bang(x) | ExprKind::RefArg(x) => collect_names_expr(x, out),
        ExprKind::Coalesce(a, b) => {
            collect_names_expr(a, out);
            collect_names_expr(b, out);
        }
        ExprKind::As(x, _) | ExprKind::Is(x, _) => collect_names_expr(x, out),
        ExprKind::Binary { left, right, .. } => {
            collect_names_expr(left, out);
            collect_names_expr(right, out);
        }
        ExprKind::Unary { operand, .. } => collect_names_expr(operand, out),
        ExprKind::If { cond, then, else_ } => {
            collect_names_expr(cond, out);
            collect_names_block(then, out);
            if let Some(eb) = else_ {
                collect_names_else(eb, out);
            }
        }
        ExprKind::IfLet { pattern, scrutinee, guard, then, else_ } => {
            collect_names_pattern(pattern, out);
            collect_names_expr(scrutinee, out);
            if let Some(g) = guard {
                collect_names_expr(g, out);
            }
            collect_names_block(then, out);
            if let Some(eb) = else_ {
                collect_names_else(eb, out);
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            collect_names_expr(scrutinee, out);
            for arm in arms {
                collect_names_pattern(&arm.pattern, out);
                if let Some(g) = &arm.guard {
                    collect_names_expr(g, out);
                }
                match &arm.body {
                    MatchArmBody::Expr(x) => collect_names_expr(x, out),
                    MatchArmBody::Block(b) => collect_names_block(b, out),
                }
            }
        }
        ExprKind::For { pattern, iter, body, invariants, decreases, .. } => {
            collect_names_pattern(pattern, out);
            collect_names_expr(iter, out);
            for inv in invariants {
                collect_names_expr(inv, out);
            }
            if let Some(d) = decreases {
                collect_names_expr(d, out);
            }
            collect_names_block(body, out);
        }
        ExprKind::ParallelFor { pattern, iter, body, .. } => {
            collect_names_pattern(pattern, out);
            collect_names_expr(iter, out);
            collect_names_block(body, out);
        }
        ExprKind::While { cond, body, invariants, decreases } => {
            collect_names_expr(cond, out);
            for inv in invariants {
                collect_names_expr(inv, out);
            }
            if let Some(d) = decreases {
                collect_names_expr(d, out);
            }
            collect_names_block(body, out);
        }
        ExprKind::WhileLet { pattern, scrutinee, guard, body, invariants, decreases } => {
            collect_names_pattern(pattern, out);
            collect_names_expr(scrutinee, out);
            if let Some(g) = guard {
                collect_names_expr(g, out);
            }
            for inv in invariants {
                collect_names_expr(inv, out);
            }
            if let Some(d) = decreases {
                collect_names_expr(d, out);
            }
            collect_names_block(body, out);
        }
        ExprKind::Loop { body, invariants, decreases } => {
            for inv in invariants {
                collect_names_expr(inv, out);
            }
            if let Some(d) = decreases {
                collect_names_expr(d, out);
            }
            collect_names_block(body, out);
        }
        ExprKind::Block(b) => collect_names_block(b, out),
        ExprKind::Spawn(x) => collect_names_expr(x, out),
        ExprKind::Detach(b) | ExprKind::Blocking(b) => collect_names_block(b, out),
        ExprKind::Supervised { body, cancel, deadline } => {
            if let Some(c) = cancel {
                collect_names_expr(c, out);
            }
            if let Some(_dl) = deadline {
                let _dl_e = &_dl.expr;
                collect_names_expr(_dl_e, out);
            }
            collect_names_block(body, out);
        }
        ExprKind::Forbid { body, .. } | ExprKind::Realtime { body, .. } => {
            collect_names_block(body, out)
        }
        ExprKind::Throw(x) => collect_names_expr(x, out),
        ExprKind::CoalesceReturnFallback(opt) => {
            if let Some(x) = opt {
                collect_names_expr(x, out);
            }
        }
        ExprKind::Interrupt(opt) => {
            if let Some(x) = opt {
                collect_names_expr(x, out);
            }
        }
        ExprKind::Range { start, end, .. } => {
            if let Some(s) = start {
                collect_names_expr(s, out);
            }
            if let Some(en) = end {
                collect_names_expr(en, out);
            }
        }
        ExprKind::TaggedTemplate { tag, args, .. } => {
            collect_names_expr(tag, out);
            for x in args {
                collect_names_expr(x, out);
            }
        }
        ExprKind::Lambda { params, body, .. } => {
            for p in params {
                out.insert(p.name.clone());
            }
            collect_names_expr(body, out);
        }
        ExprKind::ClosureLight { params, body } => {
            for p in params {
                out.insert(p.name.clone());
            }
            match body {
                ClosureBody::Expr(x) => collect_names_expr(x, out),
                ClosureBody::Block(b) => collect_names_block(b, out),
            }
        }
        ExprKind::ClosureFull(sb) => {
            for p in &sb.params {
                out.insert(p.name.clone());
            }
            match &sb.body {
                FnBody::Expr(x) => collect_names_expr(x, out),
                FnBody::Block(b) => collect_names_block(b, out),
                FnBody::External => {}
            }
        }
        ExprKind::With { bindings, body } => {
            for b in bindings {
                collect_names_expr(&b.handler, out);
            }
            collect_names_block(body, out);
        }
        ExprKind::HandlerLit { methods, .. } | ExprKind::ProtocolLit { methods, .. } => {
            for m in methods {
                for p in &m.params {
                    out.insert(p.name.clone());
                }
                match &m.body {
                    HandlerMethodBody::Expr(x) => collect_names_expr(x, out),
                    HandlerMethodBody::Block(b) => collect_names_block(b, out),
                }
            }
        }
        ExprKind::Select { arms } => {
            for arm in arms {
                match &arm.op {
                    SelectOp::Recv { binding, chan, .. } => {
                        if let Some(b) = binding {
                            out.insert(b.clone());
                        }
                        collect_names_expr(chan, out);
                    }
                    SelectOp::Send { chan, value } => {
                        collect_names_expr(chan, out);
                        collect_names_expr(value, out);
                    }
                    SelectOp::Default => {}
                }
                if let Some(g) = &arm.guard {
                    collect_names_expr(g, out);
                }
                collect_names_block(&arm.body, out);
            }
        }
        ExprKind::Forall { var, range, body } | ExprKind::Exists { var, range, body } => {
            out.insert(var.clone());
            collect_names_expr(range, out);
            collect_names_expr(body, out);
        }
    }
}

fn collect_names_else(eb: &ElseBranch, out: &mut HashSet<String>) {
    match eb {
        ElseBranch::Block(b) => collect_names_block(b, out),
        ElseBranch::If(x) => collect_names_expr(x, out),
    }
}

fn collect_names_trailing(t: &Trailing, out: &mut HashSet<String>) {
    match t {
        Trailing::Block(b) => collect_names_block(b, out),
        Trailing::LegacyBlockWithParams(tb) => {
            for p in &tb.params {
                out.insert(p.name.clone());
            }
            collect_names_block(&tb.body, out);
        }
        Trailing::Fn(sb) => {
            for p in &sb.params {
                out.insert(p.name.clone());
            }
            match &sb.body {
                FnBody::Expr(x) => collect_names_expr(x, out),
                FnBody::Block(b) => collect_names_block(b, out),
                FnBody::External => {}
            }
        }
    }
}

thread_local! {
    /// Current module's synthesized-rebind map (unique renamed name → original
    /// user base name), published by [`alpha_rename`] via [`set_demangle_map`].
    /// [`demangle_rebind_names`] consults it so it rewrites ONLY names this pass
    /// actually minted — a blind `__sN` regex would also strip a *valid* user
    /// identifier like `buf__s1` (the lexer permits it), showing the wrong name.
    static REBIND_ORIGINALS: RefCell<HashMap<String, String>> =
        RefCell::new(HashMap::new());
}

/// Publish the synthesized-name → original-name map for the current module so
/// [`demangle_rebind_names`] can demangle by exact set membership rather than a
/// name pattern (§0). Called by [`alpha_rename`] at the end of the pass;
/// **replaces** any map left by a prior compilation on this thread (one
/// compilation per thread). An empty map (no rebind) makes demangle a no-op.
pub fn set_demangle_map(originals: &HashMap<String, String>) {
    REBIND_ORIGINALS.with(|cell| {
        let mut m = cell.borrow_mut();
        m.clear();
        for (unique, orig) in originals {
            m.insert(unique.clone(), orig.clone());
        }
    });
}

/// Rewrite a rendered diagnostic message so any synthesized same-scope-rebind
/// name (`x__s1`) is shown as its original user name (`x`) — plan acceptance
/// criterion #4. Only identifiers the current [`alpha_rename`] pass actually
/// minted (present in the thread-local [`REBIND_ORIGINALS`] map) are rewritten;
/// a user identifier that merely *looks* synthesized (`buf__s1`, never minted)
/// is left intact. With no rebind in the module the map is empty and this is a
/// no-op.
pub fn demangle_rebind_names(msg: &str) -> String {
    // Cheap early-out: the reserved marker cannot be present.
    if !msg.contains("__s") {
        return msg.to_string();
    }
    REBIND_ORIGINALS.with(|cell| {
        let map = cell.borrow();
        if map.is_empty() {
            // No synthesized names for the current module → nothing was mangled;
            // leave every user identifier untouched (over-strip fix).
            return msg.to_string();
        }
        demangle_with_map(msg, &map)
    })
}

/// Replace every maximal identifier token of `msg` that is a key of `map` with
/// its mapped original name. Identifier tokens are ASCII (`[A-Za-z_][A-Za-z0-9_]*`);
/// non-identifier text (incl. multi-byte UTF-8, e.g. Russian diagnostic prose)
/// is copied verbatim by whole `char` so it is never corrupted.
fn demangle_with_map(msg: &str, map: &HashMap<String, String>) -> String {
    let mut out = String::with_capacity(msg.len());
    let mut rest = msg;
    while !rest.is_empty() {
        let bytes = rest.as_bytes();
        let first = bytes[0];
        if first == b'_' || first.is_ascii_alphabetic() {
            // Maximal ASCII identifier run.
            let mut end = 1;
            while end < bytes.len()
                && (bytes[end] == b'_' || bytes[end].is_ascii_alphanumeric())
            {
                end += 1;
            }
            let ident = &rest[..end];
            match map.get(ident) {
                Some(orig) => out.push_str(orig),
                None => out.push_str(ident),
            }
            rest = &rest[end..];
        } else {
            // Non-identifier char — copy one whole `char` (may be multi-byte).
            let ch = rest.chars().next().expect("non-empty rest");
            out.push(ch);
            rest = &rest[ch.len_utf8()..];
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;

    fn rename(src: &str) -> (Module, RebindTables) {
        let mut m = parse(src).expect("parse");
        let t = alpha_rename(&mut m);
        (m, t)
    }

    #[test]
    fn no_rebind_is_noop() {
        let (m, t) = rename("module t\nfn f() -> int {\n  ro x = 1\n  ro y = 2\n  x + y\n}\n");
        assert!(t.shadows.is_empty(), "no rebind → empty shadows");
        assert!(t.originals.is_empty());
        assert!(m.rebind_shadows.is_empty());
    }

    #[test]
    fn same_scope_rebind_uniquified() {
        let (m, t) = rename("module t\nfn f() -> int {\n  ro x = 1\n  ro x = x + 1\n  x\n}\n");
        assert_eq!(t.shadows.len(), 1, "one rebind");
        assert_eq!(m.rebind_shadows.get("x__s1").map(String::as_str), Some("x"));
        assert_eq!(t.originals.get("x__s1").map(String::as_str), Some("x"));
    }

    #[test]
    fn nested_block_shadow_not_renamed() {
        let (_m, t) = rename("module t\nfn f() -> int {\n  ro x = 1\n  { ro x = 2 }\n  x\n}\n");
        assert!(t.shadows.is_empty(), "nested shadow must NOT be uniquified");
    }

    #[test]
    fn param_shadow_uniquified() {
        // R6: a body `ro x` over parameter `x` is a same-scope rebind.
        let (m, _t) = rename("module t\nfn f(x int) -> int {\n  ro x = x + 1\n  x\n}\n");
        assert_eq!(m.rebind_shadows.get("x__s1").map(String::as_str), Some("x"));
    }

    #[test]
    fn demangle_strips_only_synthesized_names() {
        // Only names the pass actually minted are in the map.
        let mut map = HashMap::new();
        map.insert("x__s1".to_string(), "x".to_string());
        map.insert("tx__s12".to_string(), "tx".to_string());
        set_demangle_map(&map);

        assert_eq!(demangle_rebind_names("var `x__s1` bad"), "var `x` bad");
        assert_eq!(demangle_rebind_names("tx__s12 leaked"), "tx leaked");
        assert_eq!(demangle_rebind_names("no suffix here"), "no suffix here");
        // Over-strip fix: a valid user identifier that merely LOOKS synthesized
        // but was never minted (not in the map) is left intact.
        assert_eq!(demangle_rebind_names("buf__s1 fine"), "buf__s1 fine");
        // `a__s3x` is one identifier token, not in the map → intact.
        assert_eq!(demangle_rebind_names("a__s3x"), "a__s3x");
        // Multi-byte UTF-8 prose is copied verbatim, only the mapped token flips.
        assert_eq!(
            demangle_rebind_names("переменная `x__s1` жива"),
            "переменная `x` жива"
        );
    }

    #[test]
    fn demangle_noop_without_synthesized_map() {
        // No rebind in the module → empty map → nothing is stripped, even a
        // user identifier that pattern-matches the reserved suffix.
        set_demangle_map(&HashMap::new());
        assert_eq!(demangle_rebind_names("var `buf__s1` here"), "var `buf__s1` here");
    }
}
