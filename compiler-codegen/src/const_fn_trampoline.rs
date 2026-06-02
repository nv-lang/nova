//! Plan 114.4.4.3 V4.2: runtime trampoline generation для const fn first-class use.
//!
//! V3 baseline (Plan 114.4.4 Ф.2): const fn used as first-class value
//! (e.g., `ro f = double; apply(f, 5)` or `apply(double, 5)`) → friendly
//! error `E_CONST_FN_FIRST_CLASS` / `E_CONST_FN_FIRST_CLASS_RUNTIME_HOF`
//! с suggestion обернуть в lambda.
//!
//! V4.2 (this pass): автоматическая генерация runtime trampoline fn
//! `<name>__trampoline` для каждого fully-const fn, который используется
//! как first-class value. Trampoline сохраняет семантику оригинала,
//! трактуя `const` параметры как обычные runtime-параметры (всё body
//! работает за счёт того, что body fully-const fn состоит из выражений
//! над const-params, литералами и control-flow — то же самое работает
//! и в runtime). Транзитивные вызовы других const fn внутри body
//! перепиываются на `__trampoline`-имена; size_of[T]()/align_of[T]()
//! intrinsics инлайнятся литералами при генерации trampoline body.
//!
//! Pipeline placement: внутри `rewrite_const_fn_calls`, ПОСЛЕ основного
//! walker (который инлайнит const fn calls + intrinsics), ДО validation
//! и `retain` step (который дропает const fn декларации). Trampolines
//! добавляются к `module.items` под отдельным именем, поэтому переживают
//! retain (фильтрующий по оригинальным именам).
//!
//! Ограничения V1:
//! - Только fully-const fn (не mixed) eligible для trampolines.
//! - Body может содержать calls только к другим fully-const fn —
//!   они автоматически добавляются в trampoline-set транзитивно.
//! - Generic const fn unsupported (followup `[M-114.4.4-trampoline-generics]`).
//! - size_of[T]()/align_of[T]() — только primitive T (V4.0 limitation).
//! - Closure literals в body — обрабатываются Plan 114.4.4.4 (отдельно).

use std::collections::{HashMap, HashSet};

use crate::ast::{
    ArrayElem, Block, CallArg, ElseBranch, Expr, ExprKind, FnBody, FnDecl, Item, MapElem,
    MatchArmBody, Module, Stmt, Trailing, TypeRef,
};
use crate::const_fn_eval::{simple_type_name_str, type_size_or_align};
use crate::diag::Diagnostic;

const TRAMPOLINE_SUFFIX: &str = "__trampoline";

/// Trampoline pass entry. Returns `(trampoline_set, errors)`.
///
/// `trampoline_set` — имена fully-const fn (без суффикса), для которых
/// сгенерированы trampoline-декларации. Caller передаёт это в
/// `validate_const_fn_runtime_uses` чтобы подавить first-class errors
/// для этих имён.
pub fn generate_const_fn_trampolines(
    module: &mut Module,
    const_fn_decls: &HashMap<String, FnDecl>,
    aliases: &HashMap<String, String>,
) -> (HashSet<String>, Vec<Diagnostic>) {
    let mut errors: Vec<Diagnostic> = Vec::new();

    // Step 1 — collect first-class use seeds (Idents referring to const fn
    // в non-callee position).
    let const_fn_names: HashSet<String> = const_fn_decls.keys().cloned().collect();
    let mut seeds: HashSet<String> = HashSet::new();
    {
        let mut ctx = CollectCtx {
            const_fns: &const_fn_names,
            aliases,
            seeds: &mut seeds,
        };
        for item in &module.items {
            ctx.visit_item(item);
        }
        for pf in &module.peer_files {
            for item in &pf.items_here {
                ctx.visit_item(item);
            }
        }
    }
    if seeds.is_empty() {
        return (HashSet::new(), errors);
    }

    // Step 2 — transitive closure: nested Call targets inside trampoline
    // bodies должны тоже быть trampolined.
    let mut trampoline_set: HashSet<String> = seeds.clone();
    let mut worklist: Vec<String> = seeds.into_iter().collect();
    while let Some(name) = worklist.pop() {
        let Some(fd) = const_fn_decls.get(&name) else { continue };
        let mut nested: HashSet<String> = HashSet::new();
        match &fd.body {
            FnBody::Expr(e) => collect_call_targets_in_expr(e, &const_fn_names, aliases, &mut nested),
            FnBody::Block(b) => collect_call_targets_in_block(b, &const_fn_names, aliases, &mut nested),
            FnBody::External => {}
        }
        for n in nested {
            if !trampoline_set.contains(&n) {
                trampoline_set.insert(n.clone());
                worklist.push(n);
            }
        }
    }

    // Step 3 — generate trampoline FnDecls.
    let mut trampoline_items: Vec<Item> = Vec::new();
    let mut sorted_names: Vec<&String> = trampoline_set.iter().collect();
    sorted_names.sort(); // deterministic codegen order
    for name in sorted_names {
        let Some(orig) = const_fn_decls.get(name) else { continue };
        match generate_trampoline_decl(orig, &trampoline_set, &mut errors) {
            Some(td) => trampoline_items.push(Item::Fn(td)),
            None => { /* error pushed into `errors` */ }
        }
    }
    module.items.extend(trampoline_items);

    // Step 4 — rewrite first-class Ident references к trampoline names.
    {
        let mut rctx = RewriteCtx {
            const_fns: &const_fn_names,
            aliases,
            trampoline_set: &trampoline_set,
        };
        for item in &mut module.items {
            rctx.rewrite_item(item);
        }
        for pf in &mut module.peer_files {
            for item in &mut pf.items_here {
                rctx.rewrite_item(item);
            }
        }
    }

    (trampoline_set, errors)
}

/// Resolve alias chain: ALIAS → const_fn_name. Single-level (alias on alias
/// rejected by const_fn_eval).
fn resolve_alias<'a>(name: &'a str, aliases: &'a HashMap<String, String>) -> &'a str {
    aliases.get(name).map(|s| s.as_str()).unwrap_or(name)
}

// =========================================================================
// Step 1 — Collect first-class use seeds.
// =========================================================================

struct CollectCtx<'a> {
    const_fns: &'a HashSet<String>,
    aliases: &'a HashMap<String, String>,
    seeds: &'a mut HashSet<String>,
}

impl<'a> CollectCtx<'a> {
    fn maybe_seed(&mut self, e: &Expr) {
        if let ExprKind::Ident(n) = &e.kind {
            let resolved = resolve_alias(n, self.aliases);
            if self.const_fns.contains(resolved) {
                self.seeds.insert(resolved.to_string());
            }
        }
    }
    fn visit_item(&mut self, item: &Item) {
        match item {
            Item::Fn(fd) => {
                // Plan 114.4.4 Ф.2: fully-const fn bodies dropped — but their
                // body still может содержать first-class refs к ДРУГИМ const
                // fn. Однако:
                // - Если этот fn сам fully-const и dropped, его body вообще
                //   не emitted — first-class refs внутри не имеют runtime
                //   effect. Skip.
                let all_const = !fd.params.is_empty() && fd.params.iter().all(|p| p.is_const);
                let is_fully = (all_const || fd.params.is_empty()) && fd.return_is_const;
                if is_fully {
                    return;
                }
                for p in &fd.params {
                    if let Some(def) = &p.default {
                        self.visit_expr(def);
                    }
                }
                match &fd.body {
                    FnBody::Expr(e) => self.visit_expr(e),
                    FnBody::Block(b) => self.visit_block(b),
                    FnBody::External => {}
                }
            }
            Item::Const(c) => {
                // `const ALIAS = const_fn` — это alias use, не runtime
                // first-class. Skip — handled через alias map.
                if let ExprKind::Ident(n) = &c.value.kind {
                    let r = resolve_alias(n, self.aliases);
                    if self.const_fns.contains(r) {
                        return;
                    }
                }
                self.visit_expr(&c.value);
            }
            Item::Let(l) => {
                self.maybe_seed(&l.value);
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
    fn visit_block(&mut self, b: &Block) {
        for s in &b.stmts { self.visit_stmt(s); }
        if let Some(t) = &b.trailing { self.visit_expr(t); }
    }
    fn visit_stmt(&mut self, s: &Stmt) {
        match s {
            Stmt::Let(d) => {
                self.maybe_seed(&d.value);
                self.visit_expr(&d.value);
            }
            Stmt::Const(_d) => {
                // local `const ALIAS = const_fn` aliasing — like top-level.
                // Conservative: visit only non-alias values.
            }
            Stmt::Expr(e) => self.visit_expr(e),
            Stmt::Assign { target, value, .. } => {
                self.maybe_seed(value);
                self.visit_expr(target);
                self.visit_expr(value);
            }
            Stmt::Return { value: Some(v), .. } => {
                self.maybe_seed(v);
                self.visit_expr(v);
            }
            Stmt::Return { value: None, .. } => {}
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
    fn visit_expr(&mut self, e: &Expr) {
        match &e.kind {
            ExprKind::Call { func, args, trailing } => {
                // `func` — это callee, НЕ first-class use (call site).
                // Однако `func` сам может быть Ident("apply") где args содержат
                // const fn — это HOF use.
                // Не делаем `maybe_seed(func)` — это callee.
                // Recurse into func только если это сложное выражение.
                if !matches!(&func.kind, ExprKind::Ident(_)) {
                    self.visit_expr(func);
                }
                for a in args {
                    match a {
                        CallArg::Item(x) | CallArg::Spread(x) => {
                            self.maybe_seed(x);
                            self.visit_expr(x);
                        }
                        CallArg::Named { value, .. } => {
                            self.maybe_seed(value);
                            self.visit_expr(value);
                        }
                    }
                }
                if let Some(t) = trailing {
                    match t {
                        Trailing::Block(b) => self.visit_block(b),
                        Trailing::Fn(sb) => self.visit_fn_body(&sb.body),
                        Trailing::LegacyBlockWithParams(tb) => self.visit_block(&tb.body),
                    }
                }
            }
            ExprKind::Unary { operand, .. } => self.visit_expr(operand),
            ExprKind::Binary { left, right, .. } => {
                self.visit_expr(left); self.visit_expr(right);
            }
            ExprKind::As(x, _) | ExprKind::Is(x, _) => self.visit_expr(x),
            ExprKind::Try(x) | ExprKind::Bang(x) => self.visit_expr(x),
            ExprKind::Coalesce(a, b) => { self.visit_expr(a); self.visit_expr(b); }
            ExprKind::Member { obj, .. } => self.visit_expr(obj),
            ExprKind::Index { obj, index } => { self.visit_expr(obj); self.visit_expr(index); }
            ExprKind::TurboFish { base, .. } => self.visit_expr(base),
            ExprKind::TupleLit(items) => for x in items { self.visit_expr(x); },
            ExprKind::ArrayLit(elems) => for el in elems {
                match el {
                    ArrayElem::Item(x) | ArrayElem::Spread(x) => self.visit_expr(x),
                }
            },
            ExprKind::MapLit { elems, .. } => for el in elems {
                match el {
                    MapElem::Pair(k, v) => { self.visit_expr(k); self.visit_expr(v); }
                    MapElem::Spread(s) => self.visit_expr(s),
                }
            },
            ExprKind::RecordLit { fields, .. } => for f in fields {
                if let Some(v) = &f.value { self.visit_expr(v); }
            },
            ExprKind::InterpolatedStr { parts } => for p in parts {
                if let crate::ast::InterpStrPart::Expr(ex) = p { self.visit_expr(ex); }
            },
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
                        MatchArmBody::Expr(e) => self.visit_expr(e),
                        MatchArmBody::Block(b) => self.visit_block(b),
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
            ExprKind::ClosureLight { body, .. } => match body {
                crate::ast::ClosureBody::Expr(e) => self.visit_expr(e),
                crate::ast::ClosureBody::Block(b) => self.visit_block(b),
            },
            ExprKind::ClosureFull(sb) => self.visit_fn_body(&sb.body),
            ExprKind::Spawn(b) => self.visit_expr(b),
            ExprKind::Supervised { body, .. } => self.visit_block(body),
            ExprKind::Detach(b) | ExprKind::Blocking(b) => self.visit_block(b),
            ExprKind::With { body, .. } => self.visit_block(body),
            ExprKind::Forbid { body, .. } => self.visit_block(body),
            ExprKind::Realtime { body, .. } => self.visit_block(body),
            ExprKind::Interrupt(opt) => { if let Some(x) = opt { self.visit_expr(x); } }
            _ => {}
        }
    }
    fn visit_fn_body(&mut self, body: &FnBody) {
        match body {
            FnBody::Expr(e) => self.visit_expr(e),
            FnBody::Block(b) => self.visit_block(b),
            FnBody::External => {}
        }
    }
}

// =========================================================================
// Step 2 — Transitive Call-target collection (inside trampoline body).
// =========================================================================

fn collect_call_targets_in_expr(
    e: &Expr,
    const_fns: &HashSet<String>,
    aliases: &HashMap<String, String>,
    out: &mut HashSet<String>,
) {
    if let ExprKind::Call { func, args, trailing } = &e.kind {
        // Direct Call к const fn → транзитивный trampoline target.
        let callee = match &func.kind {
            ExprKind::Ident(n) => Some(n.as_str()),
            ExprKind::TurboFish { base, .. } => {
                if let ExprKind::Ident(n) = &base.kind { Some(n.as_str()) } else { None }
            }
            _ => None,
        };
        if let Some(n) = callee {
            let r = resolve_alias(n, aliases);
            if const_fns.contains(r) {
                out.insert(r.to_string());
            }
        }
        for a in args {
            match a {
                CallArg::Item(x) | CallArg::Spread(x) => collect_call_targets_in_expr(x, const_fns, aliases, out),
                CallArg::Named { value, .. } => collect_call_targets_in_expr(value, const_fns, aliases, out),
            }
        }
        if let Some(t) = trailing {
            match t {
                Trailing::Block(b) => collect_call_targets_in_block(b, const_fns, aliases, out),
                Trailing::Fn(sb) => match &sb.body {
                    FnBody::Expr(e) => collect_call_targets_in_expr(e, const_fns, aliases, out),
                    FnBody::Block(b) => collect_call_targets_in_block(b, const_fns, aliases, out),
                    FnBody::External => {}
                },
                Trailing::LegacyBlockWithParams(tb) => collect_call_targets_in_block(&tb.body, const_fns, aliases, out),
            }
        }
        return;
    }
    match &e.kind {
        ExprKind::Unary { operand, .. } => collect_call_targets_in_expr(operand, const_fns, aliases, out),
        ExprKind::Binary { left, right, .. } => {
            collect_call_targets_in_expr(left, const_fns, aliases, out);
            collect_call_targets_in_expr(right, const_fns, aliases, out);
        }
        ExprKind::As(x, _) | ExprKind::Is(x, _) => collect_call_targets_in_expr(x, const_fns, aliases, out),
        ExprKind::Try(x) | ExprKind::Bang(x) => collect_call_targets_in_expr(x, const_fns, aliases, out),
        ExprKind::Coalesce(a, b) => {
            collect_call_targets_in_expr(a, const_fns, aliases, out);
            collect_call_targets_in_expr(b, const_fns, aliases, out);
        }
        ExprKind::Member { obj, .. } => collect_call_targets_in_expr(obj, const_fns, aliases, out),
        ExprKind::Index { obj, index } => {
            collect_call_targets_in_expr(obj, const_fns, aliases, out);
            collect_call_targets_in_expr(index, const_fns, aliases, out);
        }
        ExprKind::TurboFish { base, .. } => collect_call_targets_in_expr(base, const_fns, aliases, out),
        ExprKind::TupleLit(items) => for x in items { collect_call_targets_in_expr(x, const_fns, aliases, out); },
        ExprKind::ArrayLit(elems) => for el in elems {
            match el { ArrayElem::Item(x) | ArrayElem::Spread(x) => collect_call_targets_in_expr(x, const_fns, aliases, out), }
        },
        ExprKind::RecordLit { fields, .. } => for f in fields {
            if let Some(v) = &f.value { collect_call_targets_in_expr(v, const_fns, aliases, out); }
        },
        ExprKind::If { cond, then, else_ } => {
            collect_call_targets_in_expr(cond, const_fns, aliases, out);
            collect_call_targets_in_block(then, const_fns, aliases, out);
            if let Some(eb) = else_ {
                match eb {
                    ElseBranch::Block(b) => collect_call_targets_in_block(b, const_fns, aliases, out),
                    ElseBranch::If(ie) => collect_call_targets_in_expr(ie, const_fns, aliases, out),
                }
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            collect_call_targets_in_expr(scrutinee, const_fns, aliases, out);
            for arm in arms {
                if let Some(g) = &arm.guard { collect_call_targets_in_expr(g, const_fns, aliases, out); }
                match &arm.body {
                    MatchArmBody::Expr(e) => collect_call_targets_in_expr(e, const_fns, aliases, out),
                    MatchArmBody::Block(b) => collect_call_targets_in_block(b, const_fns, aliases, out),
                }
            }
        }
        ExprKind::For { iter, body, .. } => {
            collect_call_targets_in_expr(iter, const_fns, aliases, out);
            collect_call_targets_in_block(body, const_fns, aliases, out);
        }
        ExprKind::While { cond, body, .. } => {
            collect_call_targets_in_expr(cond, const_fns, aliases, out);
            collect_call_targets_in_block(body, const_fns, aliases, out);
        }
        ExprKind::Loop { body, .. } => collect_call_targets_in_block(body, const_fns, aliases, out),
        ExprKind::Block(b) => collect_call_targets_in_block(b, const_fns, aliases, out),
        _ => {}
    }
}

fn collect_call_targets_in_block(
    b: &Block,
    const_fns: &HashSet<String>,
    aliases: &HashMap<String, String>,
    out: &mut HashSet<String>,
) {
    for s in &b.stmts {
        collect_call_targets_in_stmt(s, const_fns, aliases, out);
    }
    if let Some(t) = &b.trailing {
        collect_call_targets_in_expr(t, const_fns, aliases, out);
    }
}

fn collect_call_targets_in_stmt(
    s: &Stmt,
    const_fns: &HashSet<String>,
    aliases: &HashMap<String, String>,
    out: &mut HashSet<String>,
) {
    match s {
        Stmt::Let(d) => collect_call_targets_in_expr(&d.value, const_fns, aliases, out),
        Stmt::Const(d) => collect_call_targets_in_expr(&d.value, const_fns, aliases, out),
        Stmt::Expr(e) => collect_call_targets_in_expr(e, const_fns, aliases, out),
        Stmt::Assign { target, value, .. } => {
            collect_call_targets_in_expr(target, const_fns, aliases, out);
            collect_call_targets_in_expr(value, const_fns, aliases, out);
        }
        Stmt::Return { value: Some(v), .. } => collect_call_targets_in_expr(v, const_fns, aliases, out),
        Stmt::Throw { value, .. } => collect_call_targets_in_expr(value, const_fns, aliases, out),
        Stmt::Defer { body, .. } | Stmt::ErrDefer { body, .. }
        | Stmt::OkDefer { body, .. } | Stmt::DeferWithResult { body, .. } => {
            collect_call_targets_in_expr(body, const_fns, aliases, out);
        }
        Stmt::ConsumeScope { init, body, .. } => {
            collect_call_targets_in_expr(init, const_fns, aliases, out);
            collect_call_targets_in_block(body, const_fns, aliases, out);
        }
        Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => {
            collect_call_targets_in_expr(expr, const_fns, aliases, out);
        }
        _ => {}
    }
}

// =========================================================================
// Step 3 — Generate trampoline FnDecl.
// =========================================================================

fn trampoline_name(orig: &str) -> String {
    format!("{}{}", orig, TRAMPOLINE_SUFFIX)
}

fn generate_trampoline_decl(
    orig: &FnDecl,
    trampoline_set: &HashSet<String>,
    errors: &mut Vec<Diagnostic>,
) -> Option<FnDecl> {
    // V1: reject generic const fn (t-reflection inside trampoline body would
    // need full mono pipeline).
    if !orig.generics.is_empty() {
        errors.push(Diagnostic::new(
            format!(
                "[E_CONST_FN_TRAMPOLINE_GENERIC] generic const fn `{}` cannot be used \
                 as first-class value в V4.2 — trampoline gen requires concrete types. \
                 Followup `[M-114.4.4-trampoline-generics]`. Workaround: use concrete \
                 monomorphic wrapper.",
                orig.name
            ),
            orig.span,
        ));
        return None;
    }
    let mut td = orig.clone();
    td.name = trampoline_name(&orig.name);
    // Demote: const params → runtime params; const return → runtime return.
    for p in &mut td.params {
        p.is_const = false;
    }
    td.return_is_const = false;
    // Walk body: rewrite Call к other const fn → trampoline name; substitute
    // size_of/align_of intrinsics с literal Int.
    let local_errors = rewrite_trampoline_body(&mut td.body, trampoline_set);
    if !local_errors.is_empty() {
        errors.extend(local_errors);
        return None;
    }
    Some(td)
}

fn rewrite_trampoline_body(body: &mut FnBody, trampoline_set: &HashSet<String>) -> Vec<Diagnostic> {
    let mut errors = Vec::new();
    match body {
        FnBody::Expr(e) => rewrite_body_expr(e, trampoline_set, &mut errors),
        FnBody::Block(b) => rewrite_body_block(b, trampoline_set, &mut errors),
        FnBody::External => {}
    }
    errors
}

fn rewrite_body_block(b: &mut Block, ts: &HashSet<String>, errs: &mut Vec<Diagnostic>) {
    for s in &mut b.stmts {
        rewrite_body_stmt(s, ts, errs);
    }
    if let Some(t) = &mut b.trailing {
        rewrite_body_expr(t, ts, errs);
    }
}

fn rewrite_body_stmt(s: &mut Stmt, ts: &HashSet<String>, errs: &mut Vec<Diagnostic>) {
    match s {
        Stmt::Let(d) => rewrite_body_expr(&mut d.value, ts, errs),
        Stmt::Const(d) => rewrite_body_expr(&mut d.value, ts, errs),
        Stmt::Expr(e) => rewrite_body_expr(e, ts, errs),
        Stmt::Assign { target, value, .. } => {
            rewrite_body_expr(target, ts, errs);
            rewrite_body_expr(value, ts, errs);
        }
        Stmt::Return { value: Some(v), .. } => rewrite_body_expr(v, ts, errs),
        Stmt::Throw { value, .. } => rewrite_body_expr(value, ts, errs),
        Stmt::Defer { body, .. } | Stmt::ErrDefer { body, .. }
        | Stmt::OkDefer { body, .. } | Stmt::DeferWithResult { body, .. } => {
            rewrite_body_expr(body, ts, errs);
        }
        Stmt::ConsumeScope { init, body, .. } => {
            rewrite_body_expr(init, ts, errs);
            rewrite_body_block(body, ts, errs);
        }
        Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => {
            rewrite_body_expr(expr, ts, errs);
        }
        _ => {}
    }
}

fn rewrite_body_expr(e: &mut Expr, ts: &HashSet<String>, errs: &mut Vec<Diagnostic>) {
    // First recurse into children.
    rewrite_body_children(e, ts, errs);
    // Handle size_of[T]() / align_of[T]() intrinsics.
    let span = e.span;
    if let ExprKind::Call { func, args, trailing: None } = &e.kind {
        if let ExprKind::TurboFish { base, type_args } = &func.kind {
            if let ExprKind::Ident(n) = &base.kind {
                if (n == "size_of" || n == "align_of") && args.is_empty() && type_args.len() == 1 {
                    match type_size_or_align(&type_args[0], n == "align_of") {
                        Some(v) => {
                            *e = Expr { kind: ExprKind::IntLit(v), span };
                            return;
                        }
                        None => {
                            errs.push(Diagnostic::new(
                                format!(
                                    "[E_CONST_FN_TRAMPOLINE_INTRINSIC] `{}[T]()` для type \
                                     {} не поддерживается в V4.2 trampoline body (only \
                                     primitives). Followup `[M-114.4.4-trampoline-record-reflection]`.",
                                    n,
                                    simple_type_name_str(&type_args[0])
                                        .unwrap_or_else(|| "<complex>".to_string())
                                ),
                                span,
                            ));
                            return;
                        }
                    }
                }
            }
        }
    }
    // Handle Call к other const fn — rewrite name к trampoline.
    if let ExprKind::Call { func, .. } = &mut e.kind {
        match &mut func.kind {
            ExprKind::Ident(n) => {
                if ts.contains(n.as_str()) {
                    *n = trampoline_name(n);
                }
            }
            ExprKind::TurboFish { base, .. } => {
                if let ExprKind::Ident(n) = &mut base.kind {
                    if ts.contains(n.as_str()) {
                        *n = trampoline_name(n);
                    }
                }
            }
            _ => {}
        }
    }
}

fn rewrite_body_children(e: &mut Expr, ts: &HashSet<String>, errs: &mut Vec<Diagnostic>) {
    match &mut e.kind {
        ExprKind::Unary { operand, .. } => rewrite_body_expr(operand, ts, errs),
        ExprKind::Binary { left, right, .. } => {
            rewrite_body_expr(left, ts, errs);
            rewrite_body_expr(right, ts, errs);
        }
        ExprKind::As(x, _) | ExprKind::Is(x, _) => rewrite_body_expr(x, ts, errs),
        ExprKind::Try(x) | ExprKind::Bang(x) => rewrite_body_expr(x, ts, errs),
        ExprKind::Coalesce(a, b) => { rewrite_body_expr(a, ts, errs); rewrite_body_expr(b, ts, errs); }
        ExprKind::Member { obj, .. } => rewrite_body_expr(obj, ts, errs),
        ExprKind::Index { obj, index } => {
            rewrite_body_expr(obj, ts, errs);
            rewrite_body_expr(index, ts, errs);
        }
        ExprKind::TurboFish { base, .. } => rewrite_body_expr(base, ts, errs),
        ExprKind::Call { func, args, trailing } => {
            rewrite_body_expr(func, ts, errs);
            for a in args {
                match a {
                    CallArg::Item(x) | CallArg::Spread(x) => rewrite_body_expr(x, ts, errs),
                    CallArg::Named { value, .. } => rewrite_body_expr(value, ts, errs),
                }
            }
            if let Some(t) = trailing {
                match t {
                    Trailing::Block(b) => rewrite_body_block(b, ts, errs),
                    Trailing::Fn(sb) => match &mut sb.body {
                        FnBody::Expr(e) => rewrite_body_expr(e, ts, errs),
                        FnBody::Block(b) => rewrite_body_block(b, ts, errs),
                        FnBody::External => {}
                    },
                    Trailing::LegacyBlockWithParams(tb) => rewrite_body_block(&mut tb.body, ts, errs),
                }
            }
        }
        ExprKind::TupleLit(items) => for x in items { rewrite_body_expr(x, ts, errs); },
        ExprKind::ArrayLit(elems) => for el in elems {
            match el { ArrayElem::Item(x) | ArrayElem::Spread(x) => rewrite_body_expr(x, ts, errs), }
        },
        ExprKind::RecordLit { fields, .. } => for f in fields {
            if let Some(v) = &mut f.value { rewrite_body_expr(v, ts, errs); }
        },
        ExprKind::If { cond, then, else_ } => {
            rewrite_body_expr(cond, ts, errs);
            rewrite_body_block(then, ts, errs);
            if let Some(eb) = else_ {
                match eb {
                    ElseBranch::Block(b) => rewrite_body_block(b, ts, errs),
                    ElseBranch::If(ie) => rewrite_body_expr(ie, ts, errs),
                }
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            rewrite_body_expr(scrutinee, ts, errs);
            for arm in arms {
                if let Some(g) = &mut arm.guard { rewrite_body_expr(g, ts, errs); }
                match &mut arm.body {
                    MatchArmBody::Expr(e) => rewrite_body_expr(e, ts, errs),
                    MatchArmBody::Block(b) => rewrite_body_block(b, ts, errs),
                }
            }
        }
        ExprKind::For { iter, body, .. } => {
            rewrite_body_expr(iter, ts, errs);
            rewrite_body_block(body, ts, errs);
        }
        ExprKind::While { cond, body, .. } => {
            rewrite_body_expr(cond, ts, errs);
            rewrite_body_block(body, ts, errs);
        }
        ExprKind::Loop { body, .. } => rewrite_body_block(body, ts, errs),
        ExprKind::Block(b) => rewrite_body_block(b, ts, errs),
        _ => {}
    }
}

// =========================================================================
// Step 4 — Rewrite first-class Ident references к trampoline.
// =========================================================================

struct RewriteCtx<'a> {
    const_fns: &'a HashSet<String>,
    aliases: &'a HashMap<String, String>,
    trampoline_set: &'a HashSet<String>,
}

impl<'a> RewriteCtx<'a> {
    /// Replace `Ident(name)` in `e` if name (after alias resolution) is in
    /// trampoline_set. Returns true if replaced.
    fn maybe_rewrite(&self, e: &mut Expr) -> bool {
        if let ExprKind::Ident(n) = &e.kind {
            let resolved = resolve_alias(n, self.aliases);
            if self.trampoline_set.contains(resolved) {
                let span = e.span;
                let new_name = trampoline_name(resolved);
                *e = Expr { kind: ExprKind::Ident(new_name), span };
                return true;
            }
        }
        false
    }
    fn rewrite_item(&mut self, item: &mut Item) {
        match item {
            Item::Fn(fd) => {
                // Skip fully-const fn bodies — they're dropped.
                let all_const = !fd.params.is_empty() && fd.params.iter().all(|p| p.is_const);
                let is_fully = (all_const || fd.params.is_empty()) && fd.return_is_const;
                if is_fully {
                    return;
                }
                // Trampoline fns themselves — also skip rewriting first-class
                // refs inside (they were already rewritten in step 3 specific
                // to call-target rewrite; let it be deterministic).
                if fd.name.ends_with(TRAMPOLINE_SUFFIX) {
                    return;
                }
                for p in &mut fd.params {
                    if let Some(def) = &mut p.default {
                        self.rewrite_expr(def);
                    }
                }
                match &mut fd.body {
                    FnBody::Expr(e) => self.rewrite_expr(e),
                    FnBody::Block(b) => self.rewrite_block(b),
                    FnBody::External => {}
                }
            }
            Item::Const(c) => {
                // Skip alias-to-const-fn const decls (will be dropped по retain).
                if let ExprKind::Ident(n) = &c.value.kind {
                    let r = resolve_alias(n, self.aliases);
                    if self.const_fns.contains(r) {
                        return;
                    }
                }
                self.rewrite_expr(&mut c.value);
            }
            Item::Let(l) => {
                if !self.maybe_rewrite(&mut l.value) {
                    self.rewrite_expr(&mut l.value);
                }
            }
            Item::Test(t) => self.rewrite_block(&mut t.body),
            Item::Bench(b) => {
                for s in &mut b.setup { self.rewrite_stmt(s); }
                self.rewrite_block(&mut b.measure_body);
                for s in &mut b.teardown { self.rewrite_stmt(s); }
            }
            Item::Type(_) | Item::Lemma(_) => {}
        }
    }
    fn rewrite_block(&mut self, b: &mut Block) {
        for s in &mut b.stmts { self.rewrite_stmt(s); }
        if let Some(t) = &mut b.trailing { self.rewrite_expr(t); }
    }
    fn rewrite_stmt(&mut self, s: &mut Stmt) {
        match s {
            Stmt::Let(d) => {
                if !self.maybe_rewrite(&mut d.value) {
                    self.rewrite_expr(&mut d.value);
                }
            }
            Stmt::Const(_d) => { /* alias decls — skipped */ }
            Stmt::Expr(e) => self.rewrite_expr(e),
            Stmt::Assign { target, value, .. } => {
                self.rewrite_expr(target);
                if !self.maybe_rewrite(value) {
                    self.rewrite_expr(value);
                }
            }
            Stmt::Return { value: Some(v), .. } => {
                if !self.maybe_rewrite(v) {
                    self.rewrite_expr(v);
                }
            }
            Stmt::Return { value: None, .. } => {}
            Stmt::Throw { value, .. } => self.rewrite_expr(value),
            Stmt::Defer { body, .. } | Stmt::ErrDefer { body, .. }
            | Stmt::OkDefer { body, .. } | Stmt::DeferWithResult { body, .. } => {
                self.rewrite_expr(body);
            }
            Stmt::ConsumeScope { init, body, .. } => {
                self.rewrite_expr(init);
                self.rewrite_block(body);
            }
            Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => {
                self.rewrite_expr(expr);
            }
            _ => {}
        }
    }
    fn rewrite_expr(&mut self, e: &mut Expr) {
        match &mut e.kind {
            ExprKind::Call { func, args, trailing } => {
                // func — callee, не first-class. Recurse без maybe_rewrite.
                if !matches!(&func.kind, ExprKind::Ident(_)) {
                    self.rewrite_expr(func);
                }
                for a in args {
                    match a {
                        CallArg::Item(x) | CallArg::Spread(x) => {
                            if !self.maybe_rewrite(x) {
                                self.rewrite_expr(x);
                            }
                        }
                        CallArg::Named { value, .. } => {
                            if !self.maybe_rewrite(value) {
                                self.rewrite_expr(value);
                            }
                        }
                    }
                }
                if let Some(t) = trailing {
                    match t {
                        Trailing::Block(b) => self.rewrite_block(b),
                        Trailing::Fn(sb) => match &mut sb.body {
                            FnBody::Expr(e) => self.rewrite_expr(e),
                            FnBody::Block(b) => self.rewrite_block(b),
                            FnBody::External => {}
                        },
                        Trailing::LegacyBlockWithParams(tb) => self.rewrite_block(&mut tb.body),
                    }
                }
            }
            ExprKind::Unary { operand, .. } => self.rewrite_expr(operand),
            ExprKind::Binary { left, right, .. } => {
                self.rewrite_expr(left); self.rewrite_expr(right);
            }
            ExprKind::As(x, _) | ExprKind::Is(x, _) => self.rewrite_expr(x),
            ExprKind::Try(x) | ExprKind::Bang(x) => self.rewrite_expr(x),
            ExprKind::Coalesce(a, b) => { self.rewrite_expr(a); self.rewrite_expr(b); }
            ExprKind::Member { obj, .. } => self.rewrite_expr(obj),
            ExprKind::Index { obj, index } => { self.rewrite_expr(obj); self.rewrite_expr(index); }
            ExprKind::TurboFish { base, .. } => self.rewrite_expr(base),
            ExprKind::TupleLit(items) => for x in items { self.rewrite_expr(x); },
            ExprKind::ArrayLit(elems) => for el in elems {
                match el { ArrayElem::Item(x) | ArrayElem::Spread(x) => self.rewrite_expr(x), }
            },
            ExprKind::RecordLit { fields, .. } => for f in fields {
                if let Some(v) = &mut f.value { self.rewrite_expr(v); }
            },
            ExprKind::If { cond, then, else_ } => {
                self.rewrite_expr(cond);
                self.rewrite_block(then);
                if let Some(eb) = else_ {
                    match eb {
                        ElseBranch::Block(b) => self.rewrite_block(b),
                        ElseBranch::If(ie) => self.rewrite_expr(ie),
                    }
                }
            }
            ExprKind::Match { scrutinee, arms } => {
                self.rewrite_expr(scrutinee);
                for arm in arms {
                    if let Some(g) = &mut arm.guard { self.rewrite_expr(g); }
                    match &mut arm.body {
                        MatchArmBody::Expr(e) => self.rewrite_expr(e),
                        MatchArmBody::Block(b) => self.rewrite_block(b),
                    }
                }
            }
            ExprKind::For { iter, body, .. } => { self.rewrite_expr(iter); self.rewrite_block(body); }
            ExprKind::While { cond, body, .. } => { self.rewrite_expr(cond); self.rewrite_block(body); }
            ExprKind::Loop { body, .. } => self.rewrite_block(body),
            ExprKind::Block(b) => self.rewrite_block(b),
            ExprKind::Lambda { body, .. } => self.rewrite_expr(body),
            ExprKind::ClosureLight { body, .. } => match body {
                crate::ast::ClosureBody::Expr(e) => self.rewrite_expr(e),
                crate::ast::ClosureBody::Block(b) => self.rewrite_block(b),
            },
            ExprKind::ClosureFull(sb) => match &mut sb.body {
                FnBody::Expr(e) => self.rewrite_expr(e),
                FnBody::Block(b) => self.rewrite_block(b),
                FnBody::External => {}
            },
            ExprKind::InterpolatedStr { parts } => for p in parts {
                if let crate::ast::InterpStrPart::Expr(ex) = p { self.rewrite_expr(ex); }
            },
            ExprKind::Spawn(b) => self.rewrite_expr(b),
            ExprKind::Supervised { body, .. } => self.rewrite_block(body),
            ExprKind::Detach(b) | ExprKind::Blocking(b) => self.rewrite_block(b),
            ExprKind::With { body, .. } => self.rewrite_block(body),
            ExprKind::Forbid { body, .. } => self.rewrite_block(body),
            ExprKind::Realtime { body, .. } => self.rewrite_block(body),
            ExprKind::Interrupt(opt) => { if let Some(x) = opt { self.rewrite_expr(x); } }
            _ => {}
        }
    }
}

// Re-export for tests/external use if needed.
#[allow(dead_code)]
pub(crate) fn trampoline_suffix() -> &'static str { TRAMPOLINE_SUFFIX }

#[allow(dead_code)]
fn _unused_typeref_marker(_t: &TypeRef) {}
