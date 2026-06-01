//! Plan 123.1 (D217): Method-local receiver field caching V1 (Core CSE).
//!
//! AST-pass, который вставляет prefix-let `let _at_<F> = @<F>` в method
//! body и заменяет последующие `@<F>` reads на `_at_<F>` — устраняет
//! redundant `self->X` pointer derefs в `.c` output, **transparently**
//! сохраняя semantic equivalence (umbrella §8.1, D217 §1).
//!
//! ## Pipeline position (DECISION-A)
//!
//! `cache_module` инвокируется **ПОСЛЕ** `callnorm::normalize_module`
//! (C-codegen path) или **после** `desugar::desugar_module`
//! (interpreter path), и **ПЕРЕД** codegen / interpreter `load_module`.
//!
//! ## Scope V1 (DECISION-E)
//!
//! ONLY direct `@field` access pattern `Member { obj: SelfAccess,
//! name }`. Chains `@a.b.c` (Plan 123.4), pure-call caching `@f.m()`
//! (Plan 123.3), LICM (Plan 123.2), IPA (Plan 123.7) — отдельные
//! sub-plans.
//!
//! ## Cache classification (DECISION-C)
//!
//! - **ro field** (`RecordField.readonly == true`): unconditional cache.
//! - **mut field** (`RecordField.mutable == true`): straight-line
//!   write-region analysis (Ф.2).
//! - **consume field** (`RecordField.consume == true`): skip (D131).
//! - **embed** (`is_embed == true`): skip (use-style embed).
//!
//! ## Naming (DECISION-B, D217 §4)
//!
//! Cache local = `_at_<field>` + optional numeric suffix `_<N>` при
//! collision. Counter — per-fn (deterministic output).
//!
//! ## Edge cases (DECISION-F)
//!
//! - **Closure capture:** если ЛЮБОЙ closure body referencing `@F` →
//!   skip caching `F`.
//! - **Protocol receiver:** skip полностью (vtable dispatch).
//! - **Opaque / Effect / Sum receivers:** skip.

use crate::ast::*;
use std::collections::{HashMap, HashSet};

/// Конфигурация прохода.
#[derive(Debug, Clone, Copy)]
pub struct FieldCacheConfig {
    pub enabled: bool,
    pub threshold: usize,
    pub max_per_fn: usize,
}

impl Default for FieldCacheConfig {
    fn default() -> Self {
        Self { enabled: true, threshold: 2, max_per_fn: 8 }
    }
}

impl FieldCacheConfig {
    /// `threshold == 0` → disabled (escape hatch).
    pub fn from_threshold(threshold: usize, max_per_fn: usize) -> Self {
        if threshold == 0 {
            Self { enabled: false, threshold: 2, max_per_fn }
        } else {
            Self { enabled: true, threshold, max_per_fn }
        }
    }

    /// Read config from environment (escape hatch для test_runner /
    /// `nova test` / `nova compile` без явного CLI flag piping).
    ///
    /// Recognized env vars (all optional):
    /// - `NOVA_FIELD_CACHE=0` → pass disabled (override).
    /// - `NOVA_FIELD_CACHE_THRESHOLD=<N>` → custom threshold (default 2).
    ///   `N=0` имеет same effect как `NOVA_FIELD_CACHE=0`.
    /// - `NOVA_FIELD_CACHE_MAX=<N>` → custom per-fn cap (default 8).
    pub fn from_env_or_default() -> Self {
        let mut cfg = Self::default();
        if let Ok(v) = std::env::var("NOVA_FIELD_CACHE") {
            if v == "0" || v.eq_ignore_ascii_case("off") || v.eq_ignore_ascii_case("false") {
                cfg.enabled = false;
            }
        }
        if let Ok(v) = std::env::var("NOVA_FIELD_CACHE_THRESHOLD") {
            if let Ok(n) = v.parse::<usize>() {
                if n == 0 {
                    cfg.enabled = false;
                } else {
                    cfg.threshold = n;
                }
            }
        }
        if let Ok(v) = std::env::var("NOVA_FIELD_CACHE_MAX") {
            if let Ok(n) = v.parse::<usize>() {
                if n > 0 {
                    cfg.max_per_fn = n;
                }
            }
        }
        cfg
    }
}

/// Classification per field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FieldKind {
    /// `RecordField.readonly == true` — unconditional cache (Ф.1 path).
    Ro,
    /// `RecordField.mutable == true` — straight-line region cache
    /// (Ф.2 path). V1 implements **first-region** caching only —
    /// cache valid от body start до first write OR first call boundary;
    /// subsequent reads stay as `@F` (conservative). Full multi-region
    /// re-cache после write → `[M-123.1-mut-region-recache]` P2
    /// followup.
    Mut,
}

/// Registry: TypeName → FieldName → FieldKind.
#[derive(Debug, Default)]
struct FieldRegistry {
    by_type: HashMap<String, HashMap<String, FieldKind>>,
    /// TypeNames where receiver should be skipped entirely.
    skip_types: HashSet<String>,
}

/// Public entry-point.
pub fn cache_module(module: &mut Module, cfg: &FieldCacheConfig) {
    if !cfg.enabled {
        return;
    }
    let registry = build_registry(module);

    for item in &mut module.items {
        if let Item::Fn(f) = item {
            cache_fn(f, &registry, cfg);
        }
    }
    for pf in &mut module.peer_files {
        for item in &mut pf.items_here {
            if let Item::Fn(f) = item {
                cache_fn(f, &registry, cfg);
            }
        }
    }
}

fn build_registry(module: &Module) -> FieldRegistry {
    let mut reg = FieldRegistry::default();
    register_items(&module.items, &mut reg);
    for pf in &module.peer_files {
        register_items(&pf.items_here, &mut reg);
    }
    reg
}

fn register_items(items: &[Item], reg: &mut FieldRegistry) {
    for item in items {
        if let Item::Type(t) = item {
            match &t.kind {
                TypeDeclKind::Record(fields) => {
                    let mut map: HashMap<String, FieldKind> = HashMap::new();
                    for f in fields {
                        if f.consume || f.is_embed {
                            continue;
                        }
                        let kind = if f.readonly {
                            FieldKind::Ro
                        } else {
                            // mutable OR default: treat as Mut. Ф.1
                            // не emit'ит для них; Ф.2 — emit.
                            FieldKind::Mut
                        };
                        map.insert(f.name.clone(), kind);
                    }
                    reg.by_type.insert(t.name.clone(), map);
                }
                TypeDeclKind::NamedTuple(fields) => {
                    // D215 named tuples — stack value immutable.
                    let mut map: HashMap<String, FieldKind> = HashMap::new();
                    for f in fields {
                        map.insert(f.name.clone(), FieldKind::Ro);
                    }
                    reg.by_type.insert(t.name.clone(), map);
                }
                TypeDeclKind::Protocol { .. }
                | TypeDeclKind::Effect(_)
                | TypeDeclKind::Opaque
                | TypeDeclKind::Alias(_)
                | TypeDeclKind::Newtype(_)
                | TypeDeclKind::Sum(_) => {
                    reg.skip_types.insert(t.name.clone());
                }
            }
        }
    }
}

fn cache_fn(f: &mut FnDecl, reg: &FieldRegistry, cfg: &FieldCacheConfig) {
    let Some(recv) = &f.receiver else { return };
    let type_name = &recv.type_name;
    if reg.skip_types.contains(type_name) {
        return;
    }
    let Some(fields) = reg.by_type.get(type_name) else {
        return;
    };
    if fields.is_empty() {
        return;
    }

    let mut analysis = FnAnalysis::default();
    let body_span = match &f.body {
        FnBody::Block(b) => {
            analyze_block(b, fields, &mut analysis);
            b.span
        }
        FnBody::Expr(e) => {
            analyze_expr(e, fields, &mut analysis);
            e.span
        }
        FnBody::External => return,
    };

    // Collect in-scope local names for collision avoidance.
    let mut local_names: HashSet<String> = HashSet::new();
    for p in &f.params {
        local_names.insert(p.name.clone());
    }
    collect_local_names_fn(f, &mut local_names);

    // Split candidates по kind: ro (full-body cache) vs mut (first-
    // region cache). Both filtered by threshold + closure-capture.
    let mut field_names: Vec<&String> = analysis.read_counts.keys().collect();
    field_names.sort();
    let mut ro_candidates: Vec<(String, crate::diag::Span)> = Vec::new();
    let mut mut_candidates: Vec<(String, crate::diag::Span)> = Vec::new();
    let mut total_caches = 0usize;
    for fname in field_names {
        if total_caches >= cfg.max_per_fn {
            break;
        }
        if analysis.closure_captured.contains(fname) {
            continue;
        }
        let kind = match fields.get(fname) {
            Some(k) => *k,
            None => continue,
        };
        let global_count = analysis.read_counts.get(fname).copied().unwrap_or(0);
        let span = analysis.first_span.get(fname).copied().unwrap_or(body_span);
        match kind {
            FieldKind::Ro => {
                if global_count >= cfg.threshold {
                    ro_candidates.push((fname.clone(), span));
                    total_caches += 1;
                }
            }
            FieldKind::Mut => {
                // For mut: count reads only in first-region prefix (до
                // first write OR first call boundary). If prefix count
                // < threshold — skip.
                if let Some(prefix_count) = count_mut_prefix_reads(&f.body, fname) {
                    if prefix_count >= cfg.threshold {
                        mut_candidates.push((fname.clone(), span));
                        total_caches += 1;
                    }
                }
            }
        }
    }

    if ro_candidates.is_empty() && mut_candidates.is_empty() {
        return;
    }

    // Generate cache local names (collision-avoidance).
    let mut name_map: HashMap<String, String> = HashMap::new();
    for (fname, _) in ro_candidates.iter().chain(mut_candidates.iter()) {
        let base = format!("_at_{}", fname);
        let mut chosen = base.clone();
        let mut suffix = 0usize;
        while local_names.contains(&chosen) {
            suffix += 1;
            chosen = format!("{}_{}", base, suffix);
        }
        local_names.insert(chosen.clone());
        name_map.insert(fname.clone(), chosen);
    }

    rewrite_fn_body_split(f, &ro_candidates, &mut_candidates, &name_map);
}

/// Count `@<fname>` reads в первой straight-line prefix region body'а.
///
/// Prefix region = top-level stmts от 0 до первого stmt, который
/// содержит write to `@<fname>` OR содержит любой Call expression
/// (V1 conservative — call может mutate `@F` через alias / IPA).
/// Если body — Expr (не Block), trait как single-stmt region: count
/// reads если no write/call в этом expr.
///
/// Возвращает `None` если field receiver-typed body не applicable
/// (External / unhandled). Возвращает `Some(count)` иначе.
fn count_mut_prefix_reads(body: &FnBody, fname: &str) -> Option<usize> {
    match body {
        FnBody::Block(b) => {
            let mut count = 0usize;
            for s in &b.stmts {
                if stmt_is_barrier_for(s, fname) {
                    return Some(count);
                }
                count += count_field_reads_in_stmt(s, fname);
            }
            // No barrier in stmts → trailing also part of prefix.
            if let Some(t) = &b.trailing {
                if expr_is_barrier_for(t, fname) {
                    return Some(count);
                }
                count += count_field_reads_in_expr(t, fname);
            }
            Some(count)
        }
        FnBody::Expr(e) => {
            if expr_is_barrier_for(e, fname) {
                Some(0)
            } else {
                Some(count_field_reads_in_expr(e, fname))
            }
        }
        FnBody::External => None,
    }
}

/// True если stmt содержит write to `@<fname>` (top-level Assign on
/// `Member{SelfAccess, fname}`) OR содержит любой Call expression
/// anywhere (V1 conservative barrier).
fn stmt_is_barrier_for(s: &Stmt, fname: &str) -> bool {
    if stmt_has_write_to(s, fname) {
        return true;
    }
    stmt_contains_call(s)
}

fn expr_is_barrier_for(e: &Expr, fname: &str) -> bool {
    expr_contains_write_to(e, fname) || expr_contains_call(e)
}

fn stmt_has_write_to(s: &Stmt, fname: &str) -> bool {
    if let Stmt::Assign { target, .. } = s {
        if let Some(t_fname) = match_self_field(target) {
            if t_fname == fname {
                return true;
            }
        }
    }
    // Compound `@F[i] = v` or nested — handled below via expr walk.
    stmt_contains_write_to(s, fname)
}

fn stmt_contains_write_to(s: &Stmt, fname: &str) -> bool {
    // Conservative: any Assign with @F or @F[i] anywhere inside the
    // stmt's expressions is a "write" for cache-invalidation purposes.
    match s {
        Stmt::Assign { target, value, .. } => {
            expr_contains_write_to(target, fname) || expr_contains_write_to(value, fname)
                || (match_self_field(target) == Some(fname))
        }
        Stmt::Let(d) => expr_contains_write_to(&d.value, fname),
        Stmt::Const(d) => expr_contains_write_to(&d.value, fname),
        Stmt::Expr(e) => expr_contains_write_to(e, fname),
        Stmt::Return { value, .. } => value.as_ref().map_or(false, |v| expr_contains_write_to(v, fname)),
        Stmt::Throw { value, .. } => expr_contains_write_to(value, fname),
        Stmt::Defer { body, .. } | Stmt::ErrDefer { body, .. }
        | Stmt::OkDefer { body, .. } | Stmt::DeferWithResult { body, .. } => {
            expr_contains_write_to(body, fname)
        }
        Stmt::ConsumeScope { init, body, .. } => {
            expr_contains_write_to(init, fname) || block_contains_write_to(body, fname)
        }
        Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => {
            expr_contains_write_to(expr, fname)
        }
        Stmt::Break(_) | Stmt::Continue(_)
        | Stmt::Apply { .. } | Stmt::Calc { .. } | Stmt::Reveal { .. } => false,
    }
}

fn block_contains_write_to(b: &Block, fname: &str) -> bool {
    b.stmts.iter().any(|s| stmt_contains_write_to(s, fname))
        || b.trailing.as_ref().map_or(false, |t| expr_contains_write_to(t, fname))
}

fn expr_contains_write_to(e: &Expr, fname: &str) -> bool {
    // Walk all sub-exprs / sub-stmts; look for any control-flow Block
    // with an Assign Stmt где target = `@<fname>` (or indexed @F).
    match &e.kind {
        ExprKind::Block(b) => block_contains_write_to(b, fname),
        ExprKind::If { cond, then, else_ } => {
            expr_contains_write_to(cond, fname)
                || block_contains_write_to(then, fname)
                || else_.as_ref().map_or(false, |eb| else_branch_contains_write_to(eb, fname))
        }
        ExprKind::IfLet { scrutinee, then, else_, .. } => {
            expr_contains_write_to(scrutinee, fname)
                || block_contains_write_to(then, fname)
                || else_.as_ref().map_or(false, |eb| else_branch_contains_write_to(eb, fname))
        }
        ExprKind::Match { scrutinee, arms } => {
            expr_contains_write_to(scrutinee, fname)
                || arms.iter().any(|arm| {
                    arm.guard.as_ref().map_or(false, |g| expr_contains_write_to(g, fname))
                        || match &arm.body {
                            MatchArmBody::Expr(e) => expr_contains_write_to(e, fname),
                            MatchArmBody::Block(b) => block_contains_write_to(b, fname),
                        }
                })
        }
        ExprKind::For { iter, body, .. } | ExprKind::ParallelFor { iter, body, .. } => {
            expr_contains_write_to(iter, fname) || block_contains_write_to(body, fname)
        }
        ExprKind::While { cond, body, .. } => {
            expr_contains_write_to(cond, fname) || block_contains_write_to(body, fname)
        }
        ExprKind::WhileLet { scrutinee, body, .. } => {
            expr_contains_write_to(scrutinee, fname) || block_contains_write_to(body, fname)
        }
        ExprKind::Loop { body, .. } => block_contains_write_to(body, fname),
        ExprKind::With { bindings, body } => {
            bindings.iter().any(|wb| expr_contains_write_to(&wb.handler, fname))
                || block_contains_write_to(body, fname)
        }
        ExprKind::Forbid { body, .. } | ExprKind::Realtime { body, .. }
        | ExprKind::Detach(body) | ExprKind::Blocking(body) => {
            block_contains_write_to(body, fname)
        }
        ExprKind::Supervised { body, cancel } => {
            block_contains_write_to(body, fname)
                || cancel.as_ref().map_or(false, |c| expr_contains_write_to(c, fname))
        }
        ExprKind::Spawn(e) | ExprKind::Throw(e) => expr_contains_write_to(e, fname),
        ExprKind::Try(e) | ExprKind::Bang(e) | ExprKind::Member { obj: e, .. }
        | ExprKind::TurboFish { base: e, .. } | ExprKind::As(e, _) | ExprKind::Is(e, _)
        | ExprKind::Unary { operand: e, .. } => expr_contains_write_to(e, fname),
        ExprKind::Coalesce(a, b) | ExprKind::Binary { left: a, right: b, .. } => {
            expr_contains_write_to(a, fname) || expr_contains_write_to(b, fname)
        }
        ExprKind::Index { obj, index } => {
            expr_contains_write_to(obj, fname) || expr_contains_write_to(index, fname)
        }
        ExprKind::Call { func, args, trailing } => {
            expr_contains_write_to(func, fname)
                || args.iter().any(|a| expr_contains_write_to(a.expr(), fname))
                || trailing.as_ref().map_or(false, |t| trailing_contains_write_to(t, fname))
        }
        ExprKind::ArrayLit(elems) => elems.iter().any(|el| match el {
            ArrayElem::Item(e) | ArrayElem::Spread(e) => expr_contains_write_to(e, fname),
        }),
        ExprKind::MapLit { elems, .. } => elems.iter().any(|el| match el {
            MapElem::Pair(k, v) => expr_contains_write_to(k, fname) || expr_contains_write_to(v, fname),
            MapElem::Spread(e) => expr_contains_write_to(e, fname),
        }),
        ExprKind::RecordLit { fields: rfields, .. } => rfields.iter().any(|rf| {
            rf.value.as_ref().map_or(false, |v| expr_contains_write_to(v, fname))
        }),
        ExprKind::TupleLit(elems) => elems.iter().any(|el| expr_contains_write_to(el, fname)),
        ExprKind::InterpolatedStr { parts } => parts.iter().any(|p| {
            if let InterpStrPart::Expr(e) = p { expr_contains_write_to(e, fname) } else { false }
        }),
        ExprKind::Select { arms } => arms.iter().any(|arm| {
            block_contains_write_to(&arm.body, fname)
                || arm.guard.as_ref().map_or(false, |g| expr_contains_write_to(g, fname))
                || match &arm.op {
                    SelectOp::Recv { chan, .. } => expr_contains_write_to(chan, fname),
                    SelectOp::Send { chan, value } => expr_contains_write_to(chan, fname) || expr_contains_write_to(value, fname),
                    SelectOp::Default => false,
                }
        }),
        ExprKind::Range { start, end, .. } => {
            start.as_ref().map_or(false, |s| expr_contains_write_to(s, fname))
                || end.as_ref().map_or(false, |e| expr_contains_write_to(e, fname))
        }
        ExprKind::Forall { range, body, .. } | ExprKind::Exists { range, body, .. } => {
            expr_contains_write_to(range, fname) || expr_contains_write_to(body, fname)
        }
        ExprKind::Interrupt(opt) => opt.as_ref().map_or(false, |e| expr_contains_write_to(e, fname)),
        ExprKind::TaggedTemplate { tag, args, .. } => {
            expr_contains_write_to(tag, fname)
                || args.iter().any(|a| expr_contains_write_to(a, fname))
        }
        // Closures: their bodies could contain writes, but those are
        // separate scope. V1 conservative: closures already cause
        // skip-caching for captured fields (closure_captured).
        // Here treat as no barrier (closures don't execute synchronously
        // — they're values, не immediate execution).
        ExprKind::Lambda { .. } | ExprKind::ClosureLight { .. }
        | ExprKind::ClosureFull(_) | ExprKind::HandlerLit { .. }
        | ExprKind::ProtocolLit { .. } => false,
        ExprKind::IntLit(_) | ExprKind::FloatLit(_) | ExprKind::StrLit(_)
        | ExprKind::BoolLit(_) | ExprKind::UnitLit | ExprKind::CharLit(_)
        | ExprKind::NullPtrLit | ExprKind::Ident(_) | ExprKind::Path(_)
        | ExprKind::SelfAccess => false,
    }
}

fn else_branch_contains_write_to(eb: &ElseBranch, fname: &str) -> bool {
    match eb {
        ElseBranch::Block(b) => block_contains_write_to(b, fname),
        ElseBranch::If(e) => expr_contains_write_to(e, fname),
    }
}

fn trailing_contains_write_to(t: &Trailing, fname: &str) -> bool {
    match t {
        Trailing::Block(b) => block_contains_write_to(b, fname),
        Trailing::Fn(sb) => match &sb.body {
            FnBody::Expr(e) => expr_contains_write_to(e, fname),
            FnBody::Block(b) => block_contains_write_to(b, fname),
            FnBody::External => false,
        },
        Trailing::LegacyBlockWithParams(tb) => block_contains_write_to(&tb.body, fname),
    }
}

/// True если stmt syntactically contains any `ExprKind::Call` —
/// V1 conservative barrier для mut field caching.
fn stmt_contains_call(s: &Stmt) -> bool {
    match s {
        Stmt::Let(d) => expr_contains_call(&d.value),
        Stmt::Const(d) => expr_contains_call(&d.value),
        Stmt::Expr(e) => expr_contains_call(e),
        Stmt::Assign { target, value, .. } => {
            expr_contains_call(target) || expr_contains_call(value)
        }
        Stmt::Return { value, .. } => value.as_ref().map_or(false, |v| expr_contains_call(v)),
        Stmt::Throw { value, .. } => expr_contains_call(value),
        Stmt::Defer { body, .. } | Stmt::ErrDefer { body, .. }
        | Stmt::OkDefer { body, .. } | Stmt::DeferWithResult { body, .. } => {
            expr_contains_call(body)
        }
        Stmt::ConsumeScope { init, body, .. } => {
            expr_contains_call(init) || block_contains_call(body)
        }
        Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => expr_contains_call(expr),
        Stmt::Break(_) | Stmt::Continue(_)
        | Stmt::Apply { .. } | Stmt::Calc { .. } | Stmt::Reveal { .. } => false,
    }
}

fn block_contains_call(b: &Block) -> bool {
    b.stmts.iter().any(stmt_contains_call) || b.trailing.as_ref().map_or(false, |t| expr_contains_call(t))
}

fn expr_contains_call(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Call { .. } => true,
        // Don't treat method-call sugar — already a Call.
        // Spawn / Supervised / etc. — their body is async, не immediate
        // execution. Still V1 conservative: their body may execute и
        // mutate via aliased self. Treat as barrier для safety.
        ExprKind::Spawn(_) | ExprKind::Supervised { .. } | ExprKind::Detach(_)
        | ExprKind::Blocking(_) => true,
        // `with` body может invoke handler — treat as barrier.
        ExprKind::With { .. } => true,
        // Compound walks below.
        ExprKind::Block(b) => block_contains_call(b),
        ExprKind::If { cond, then, else_ } => {
            expr_contains_call(cond) || block_contains_call(then)
                || else_.as_ref().map_or(false, |eb| else_branch_contains_call(eb))
        }
        ExprKind::IfLet { scrutinee, then, else_, .. } => {
            expr_contains_call(scrutinee) || block_contains_call(then)
                || else_.as_ref().map_or(false, |eb| else_branch_contains_call(eb))
        }
        ExprKind::Match { scrutinee, arms } => {
            expr_contains_call(scrutinee) || arms.iter().any(|arm| {
                arm.guard.as_ref().map_or(false, |g| expr_contains_call(g))
                    || match &arm.body {
                        MatchArmBody::Expr(e) => expr_contains_call(e),
                        MatchArmBody::Block(b) => block_contains_call(b),
                    }
            })
        }
        ExprKind::For { iter, body, .. } | ExprKind::ParallelFor { iter, body, .. } => {
            expr_contains_call(iter) || block_contains_call(body)
        }
        ExprKind::While { cond, body, .. } => {
            expr_contains_call(cond) || block_contains_call(body)
        }
        ExprKind::WhileLet { scrutinee, body, .. } => {
            expr_contains_call(scrutinee) || block_contains_call(body)
        }
        ExprKind::Loop { body, .. } => block_contains_call(body),
        ExprKind::Forbid { body, .. } | ExprKind::Realtime { body, .. } => {
            block_contains_call(body)
        }
        ExprKind::Throw(e) => expr_contains_call(e),
        ExprKind::Try(e) | ExprKind::Bang(e) | ExprKind::Member { obj: e, .. }
        | ExprKind::TurboFish { base: e, .. } | ExprKind::As(e, _) | ExprKind::Is(e, _)
        | ExprKind::Unary { operand: e, .. } => expr_contains_call(e),
        ExprKind::Coalesce(a, b) | ExprKind::Binary { left: a, right: b, .. } => {
            expr_contains_call(a) || expr_contains_call(b)
        }
        ExprKind::Index { obj, index } => expr_contains_call(obj) || expr_contains_call(index),
        ExprKind::ArrayLit(elems) => elems.iter().any(|el| match el {
            ArrayElem::Item(e) | ArrayElem::Spread(e) => expr_contains_call(e),
        }),
        ExprKind::MapLit { elems, .. } => elems.iter().any(|el| match el {
            MapElem::Pair(k, v) => expr_contains_call(k) || expr_contains_call(v),
            MapElem::Spread(e) => expr_contains_call(e),
        }),
        ExprKind::RecordLit { fields: rfields, .. } => rfields.iter().any(|rf| {
            rf.value.as_ref().map_or(false, |v| expr_contains_call(v))
        }),
        ExprKind::TupleLit(elems) => elems.iter().any(|el| expr_contains_call(el)),
        ExprKind::InterpolatedStr { parts } => parts.iter().any(|p| {
            if let InterpStrPart::Expr(e) = p { expr_contains_call(e) } else { false }
        }),
        ExprKind::Select { .. } => true, // channel ops effectively are call/blocking
        ExprKind::Range { start, end, .. } => {
            start.as_ref().map_or(false, |s| expr_contains_call(s))
                || end.as_ref().map_or(false, |e| expr_contains_call(e))
        }
        ExprKind::Forall { range, body, .. } | ExprKind::Exists { range, body, .. } => {
            expr_contains_call(range) || expr_contains_call(body)
        }
        ExprKind::Interrupt(opt) => opt.as_ref().map_or(false, |e| expr_contains_call(e)),
        ExprKind::TaggedTemplate { tag, args, .. } => {
            expr_contains_call(tag) || args.iter().any(expr_contains_call)
        }
        // Closures — values, не immediate execution. Не barrier.
        ExprKind::Lambda { .. } | ExprKind::ClosureLight { .. }
        | ExprKind::ClosureFull(_) | ExprKind::HandlerLit { .. }
        | ExprKind::ProtocolLit { .. } => false,
        ExprKind::IntLit(_) | ExprKind::FloatLit(_) | ExprKind::StrLit(_)
        | ExprKind::BoolLit(_) | ExprKind::UnitLit | ExprKind::CharLit(_)
        | ExprKind::NullPtrLit | ExprKind::Ident(_) | ExprKind::Path(_)
        | ExprKind::SelfAccess => false,
    }
}

fn else_branch_contains_call(eb: &ElseBranch) -> bool {
    match eb {
        ElseBranch::Block(b) => block_contains_call(b),
        ElseBranch::If(e) => expr_contains_call(e),
    }
}

/// Count `@<fname>` reads in a single Stmt (recursive into sub-exprs).
fn count_field_reads_in_stmt(s: &Stmt, fname: &str) -> usize {
    match s {
        Stmt::Let(d) => count_field_reads_in_expr(&d.value, fname),
        Stmt::Const(d) => count_field_reads_in_expr(&d.value, fname),
        Stmt::Expr(e) => count_field_reads_in_expr(e, fname),
        Stmt::Assign { target, value, .. } => {
            let t_count = if match_self_field(target).is_some() {
                0 // top-level @F = ... — target is write, not read.
            } else {
                count_field_reads_in_expr(target, fname)
            };
            t_count + count_field_reads_in_expr(value, fname)
        }
        Stmt::Return { value, .. } => value.as_ref().map_or(0, |v| count_field_reads_in_expr(v, fname)),
        Stmt::Throw { value, .. } => count_field_reads_in_expr(value, fname),
        Stmt::Defer { body, .. } | Stmt::ErrDefer { body, .. }
        | Stmt::OkDefer { body, .. } | Stmt::DeferWithResult { body, .. } => {
            count_field_reads_in_expr(body, fname)
        }
        Stmt::ConsumeScope { init, body, .. } => {
            count_field_reads_in_expr(init, fname) + count_field_reads_in_block(body, fname)
        }
        Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => {
            count_field_reads_in_expr(expr, fname)
        }
        Stmt::Break(_) | Stmt::Continue(_)
        | Stmt::Apply { .. } | Stmt::Calc { .. } | Stmt::Reveal { .. } => 0,
    }
}

fn count_field_reads_in_block(b: &Block, fname: &str) -> usize {
    let mut c = 0;
    for s in &b.stmts {
        c += count_field_reads_in_stmt(s, fname);
    }
    if let Some(t) = &b.trailing {
        c += count_field_reads_in_expr(t, fname);
    }
    c
}

fn count_field_reads_in_expr(e: &Expr, fname: &str) -> usize {
    if let Some(t_fname) = match_self_field(e) {
        return if t_fname == fname { 1 } else { 0 };
    }
    // Don't count inside closures (separate scope).
    if matches!(&e.kind,
        ExprKind::Lambda { .. } | ExprKind::ClosureLight { .. }
        | ExprKind::ClosureFull(_) | ExprKind::HandlerLit { .. }
        | ExprKind::ProtocolLit { .. }
    ) {
        return 0;
    }
    let mut c = 0;
    match &e.kind {
        ExprKind::Block(b) => c += count_field_reads_in_block(b, fname),
        ExprKind::If { cond, then, else_ } => {
            c += count_field_reads_in_expr(cond, fname);
            c += count_field_reads_in_block(then, fname);
            if let Some(eb) = else_ {
                c += match eb {
                    ElseBranch::Block(b) => count_field_reads_in_block(b, fname),
                    ElseBranch::If(e) => count_field_reads_in_expr(e, fname),
                };
            }
        }
        ExprKind::IfLet { scrutinee, then, else_, .. } => {
            c += count_field_reads_in_expr(scrutinee, fname);
            c += count_field_reads_in_block(then, fname);
            if let Some(eb) = else_ {
                c += match eb {
                    ElseBranch::Block(b) => count_field_reads_in_block(b, fname),
                    ElseBranch::If(e) => count_field_reads_in_expr(e, fname),
                };
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            c += count_field_reads_in_expr(scrutinee, fname);
            for arm in arms {
                if let Some(g) = &arm.guard {
                    c += count_field_reads_in_expr(g, fname);
                }
                c += match &arm.body {
                    MatchArmBody::Expr(e) => count_field_reads_in_expr(e, fname),
                    MatchArmBody::Block(b) => count_field_reads_in_block(b, fname),
                };
            }
        }
        ExprKind::For { iter, body, .. } | ExprKind::ParallelFor { iter, body, .. } => {
            c += count_field_reads_in_expr(iter, fname);
            c += count_field_reads_in_block(body, fname);
        }
        ExprKind::While { cond, body, .. } => {
            c += count_field_reads_in_expr(cond, fname);
            c += count_field_reads_in_block(body, fname);
        }
        ExprKind::WhileLet { scrutinee, body, .. } => {
            c += count_field_reads_in_expr(scrutinee, fname);
            c += count_field_reads_in_block(body, fname);
        }
        ExprKind::Loop { body, .. } => c += count_field_reads_in_block(body, fname),
        ExprKind::With { bindings, body } => {
            for wb in bindings {
                c += count_field_reads_in_expr(&wb.handler, fname);
            }
            c += count_field_reads_in_block(body, fname);
        }
        ExprKind::Forbid { body, .. } | ExprKind::Realtime { body, .. }
        | ExprKind::Detach(body) | ExprKind::Blocking(body) => {
            c += count_field_reads_in_block(body, fname);
        }
        ExprKind::Supervised { body, cancel } => {
            c += count_field_reads_in_block(body, fname);
            if let Some(cc) = cancel {
                c += count_field_reads_in_expr(cc, fname);
            }
        }
        ExprKind::Spawn(e) | ExprKind::Throw(e) => c += count_field_reads_in_expr(e, fname),
        ExprKind::Try(e) | ExprKind::Bang(e) | ExprKind::Member { obj: e, .. }
        | ExprKind::TurboFish { base: e, .. } | ExprKind::As(e, _) | ExprKind::Is(e, _)
        | ExprKind::Unary { operand: e, .. } => c += count_field_reads_in_expr(e, fname),
        ExprKind::Coalesce(a, b) | ExprKind::Binary { left: a, right: b, .. } => {
            c += count_field_reads_in_expr(a, fname);
            c += count_field_reads_in_expr(b, fname);
        }
        ExprKind::Index { obj, index } => {
            c += count_field_reads_in_expr(obj, fname);
            c += count_field_reads_in_expr(index, fname);
        }
        ExprKind::Call { func, args, trailing } => {
            c += count_field_reads_in_expr(func, fname);
            for arg in args {
                c += count_field_reads_in_expr(arg.expr(), fname);
            }
            if let Some(t) = trailing {
                c += match t {
                    Trailing::Block(b) => count_field_reads_in_block(b, fname),
                    Trailing::Fn(sb) => match &sb.body {
                        FnBody::Expr(e) => count_field_reads_in_expr(e, fname),
                        FnBody::Block(b) => count_field_reads_in_block(b, fname),
                        FnBody::External => 0,
                    },
                    Trailing::LegacyBlockWithParams(tb) => count_field_reads_in_block(&tb.body, fname),
                };
            }
        }
        ExprKind::ArrayLit(elems) => {
            for el in elems {
                c += match el {
                    ArrayElem::Item(e) | ArrayElem::Spread(e) => count_field_reads_in_expr(e, fname),
                };
            }
        }
        ExprKind::MapLit { elems, .. } => {
            for el in elems {
                c += match el {
                    MapElem::Pair(k, v) => count_field_reads_in_expr(k, fname) + count_field_reads_in_expr(v, fname),
                    MapElem::Spread(e) => count_field_reads_in_expr(e, fname),
                };
            }
        }
        ExprKind::RecordLit { fields: rfields, .. } => {
            for rf in rfields {
                if let Some(v) = &rf.value {
                    c += count_field_reads_in_expr(v, fname);
                }
            }
        }
        ExprKind::TupleLit(elems) => {
            for el in elems {
                c += count_field_reads_in_expr(el, fname);
            }
        }
        ExprKind::InterpolatedStr { parts } => {
            for p in parts {
                if let InterpStrPart::Expr(e) = p {
                    c += count_field_reads_in_expr(e, fname);
                }
            }
        }
        ExprKind::Select { arms } => {
            for arm in arms {
                if let Some(g) = &arm.guard {
                    c += count_field_reads_in_expr(g, fname);
                }
                c += count_field_reads_in_block(&arm.body, fname);
                c += match &arm.op {
                    SelectOp::Recv { chan, .. } => count_field_reads_in_expr(chan, fname),
                    SelectOp::Send { chan, value } => count_field_reads_in_expr(chan, fname) + count_field_reads_in_expr(value, fname),
                    SelectOp::Default => 0,
                };
            }
        }
        ExprKind::Range { start, end, .. } => {
            if let Some(s) = start { c += count_field_reads_in_expr(s, fname); }
            if let Some(e) = end { c += count_field_reads_in_expr(e, fname); }
        }
        ExprKind::Forall { range, body, .. } | ExprKind::Exists { range, body, .. } => {
            c += count_field_reads_in_expr(range, fname);
            c += count_field_reads_in_expr(body, fname);
        }
        ExprKind::Interrupt(opt) => {
            if let Some(e) = opt { c += count_field_reads_in_expr(e, fname); }
        }
        ExprKind::TaggedTemplate { tag, args, .. } => {
            c += count_field_reads_in_expr(tag, fname);
            for arg in args {
                c += count_field_reads_in_expr(arg, fname);
            }
        }
        _ => {}
    }
    c
}

#[derive(Debug, Default)]
struct FnAnalysis {
    read_counts: HashMap<String, usize>,
    closure_captured: HashSet<String>,
    #[allow(dead_code)]
    written: HashSet<String>,
    first_span: HashMap<String, crate::diag::Span>,
}

fn analyze_block(b: &Block, fields: &HashMap<String, FieldKind>, a: &mut FnAnalysis) {
    for s in &b.stmts {
        analyze_stmt(s, fields, a);
    }
    if let Some(t) = &b.trailing {
        analyze_expr(t, fields, a);
    }
}

fn analyze_stmt(s: &Stmt, fields: &HashMap<String, FieldKind>, a: &mut FnAnalysis) {
    match s {
        Stmt::Let(d) => analyze_expr(&d.value, fields, a),
        Stmt::Const(d) => analyze_expr(&d.value, fields, a),
        Stmt::Expr(e) => analyze_expr(e, fields, a),
        Stmt::Assign { target, value, .. } => {
            if let Some(fname) = match_self_field(target) {
                if fields.contains_key(fname) {
                    a.written.insert(fname.to_string());
                }
                // top-level `@F = ...` — target sub-expr (self) — already
                // accounted via the assignment itself. Don't recurse
                // into target as a read.
            } else {
                analyze_expr(target, fields, a);
            }
            analyze_expr(value, fields, a);
        }
        Stmt::Return { value, .. } => {
            if let Some(v) = value {
                analyze_expr(v, fields, a);
            }
        }
        Stmt::Throw { value, .. } => analyze_expr(value, fields, a),
        Stmt::Break(_) | Stmt::Continue(_) => {}
        Stmt::Defer { body, .. }
        | Stmt::ErrDefer { body, .. }
        | Stmt::OkDefer { body, .. }
        | Stmt::DeferWithResult { body, .. } => analyze_expr(body, fields, a),
        Stmt::ConsumeScope { init, body, .. } => {
            analyze_expr(init, fields, a);
            analyze_block(body, fields, a);
        }
        Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => {
            analyze_expr(expr, fields, a);
        }
        Stmt::Apply { .. } | Stmt::Calc { .. } | Stmt::Reveal { .. } => {}
    }
}

fn match_self_field(e: &Expr) -> Option<&str> {
    if let ExprKind::Member { obj, name } = &e.kind {
        if matches!(obj.kind, ExprKind::SelfAccess) {
            return Some(name.as_str());
        }
    }
    None
}

fn analyze_expr(e: &Expr, fields: &HashMap<String, FieldKind>, a: &mut FnAnalysis) {
    // `@F` read detection.
    if let Some(fname) = match_self_field(e) {
        if fields.contains_key(fname) {
            *a.read_counts.entry(fname.to_string()).or_insert(0) += 1;
            a.first_span.entry(fname.to_string()).or_insert(e.span);
        }
        return;
    }

    // Closure detection: any closure body referencing `@F` → captured.
    match &e.kind {
        ExprKind::ClosureLight { body, .. } => {
            scan_closure_body(body, fields, &mut a.closure_captured);
            return;
        }
        ExprKind::ClosureFull(b) => {
            scan_fn_body(&b.body, fields, &mut a.closure_captured);
            return;
        }
        ExprKind::Lambda { body, .. } => {
            scan_expr(body, fields, &mut a.closure_captured);
            return;
        }
        ExprKind::HandlerLit { methods, .. } | ExprKind::ProtocolLit { methods, .. } => {
            for m in methods {
                match &m.body {
                    HandlerMethodBody::Expr(e) => scan_expr(e, fields, &mut a.closure_captured),
                    HandlerMethodBody::Block(b) => scan_block(b, fields, &mut a.closure_captured),
                }
            }
            return;
        }
        _ => {}
    }

    analyze_expr_children(e, fields, a);
}

fn analyze_expr_children(e: &Expr, fields: &HashMap<String, FieldKind>, a: &mut FnAnalysis) {
    match &e.kind {
        ExprKind::IntLit(_) | ExprKind::FloatLit(_) | ExprKind::StrLit(_)
        | ExprKind::BoolLit(_) | ExprKind::UnitLit | ExprKind::CharLit(_)
        | ExprKind::NullPtrLit | ExprKind::Ident(_) | ExprKind::Path(_)
        | ExprKind::SelfAccess => {}

        ExprKind::InterpolatedStr { parts } => {
            for p in parts {
                if let InterpStrPart::Expr(e) = p {
                    analyze_expr(e, fields, a);
                }
            }
        }
        ExprKind::ArrayLit(elems) => {
            for el in elems {
                match el {
                    ArrayElem::Item(e) | ArrayElem::Spread(e) => analyze_expr(e, fields, a),
                }
            }
        }
        ExprKind::MapLit { elems, .. } => {
            for el in elems {
                match el {
                    MapElem::Pair(k, v) => {
                        analyze_expr(k, fields, a);
                        analyze_expr(v, fields, a);
                    }
                    MapElem::Spread(e) => analyze_expr(e, fields, a),
                }
            }
        }
        ExprKind::RecordLit { fields: rfields, .. } => {
            for rf in rfields {
                if let Some(v) = &rf.value {
                    analyze_expr(v, fields, a);
                }
            }
        }
        ExprKind::TupleLit(elems) => {
            for el in elems {
                analyze_expr(el, fields, a);
            }
        }
        ExprKind::Member { obj, .. } => analyze_expr(obj, fields, a),
        ExprKind::Index { obj, index } => {
            analyze_expr(obj, fields, a);
            analyze_expr(index, fields, a);
        }
        ExprKind::TurboFish { base, .. } => analyze_expr(base, fields, a),
        ExprKind::Call { func, args, trailing } => {
            analyze_expr(func, fields, a);
            for arg in args {
                match arg {
                    CallArg::Item(e) | CallArg::Spread(e) => analyze_expr(e, fields, a),
                    CallArg::Named { value, .. } => analyze_expr(value, fields, a),
                }
            }
            if let Some(t) = trailing {
                analyze_trailing(t, fields, a);
            }
        }
        ExprKind::Try(e) | ExprKind::Bang(e) => analyze_expr(e, fields, a),
        ExprKind::Coalesce(a_e, b_e) => {
            analyze_expr(a_e, fields, a);
            analyze_expr(b_e, fields, a);
        }
        ExprKind::As(e, _) | ExprKind::Is(e, _) => analyze_expr(e, fields, a),
        ExprKind::Binary { left, right, .. } => {
            analyze_expr(left, fields, a);
            analyze_expr(right, fields, a);
        }
        ExprKind::Unary { operand, .. } => analyze_expr(operand, fields, a),
        ExprKind::If { cond, then, else_ } => {
            analyze_expr(cond, fields, a);
            analyze_block(then, fields, a);
            if let Some(eb) = else_ {
                analyze_else(eb, fields, a);
            }
        }
        ExprKind::IfLet { scrutinee, then, else_, .. } => {
            analyze_expr(scrutinee, fields, a);
            analyze_block(then, fields, a);
            if let Some(eb) = else_ {
                analyze_else(eb, fields, a);
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            analyze_expr(scrutinee, fields, a);
            for arm in arms {
                if let Some(g) = &arm.guard {
                    analyze_expr(g, fields, a);
                }
                match &arm.body {
                    MatchArmBody::Expr(e) => analyze_expr(e, fields, a),
                    MatchArmBody::Block(b) => analyze_block(b, fields, a),
                }
            }
        }
        ExprKind::For { iter, body, invariants, decreases, .. } => {
            analyze_expr(iter, fields, a);
            analyze_block(body, fields, a);
            for inv in invariants {
                analyze_expr(inv, fields, a);
            }
            if let Some(d) = decreases {
                analyze_expr(d, fields, a);
            }
        }
        ExprKind::ParallelFor { iter, body, .. } => {
            analyze_expr(iter, fields, a);
            analyze_block(body, fields, a);
        }
        ExprKind::While { cond, body, invariants, decreases } => {
            analyze_expr(cond, fields, a);
            analyze_block(body, fields, a);
            for inv in invariants {
                analyze_expr(inv, fields, a);
            }
            if let Some(d) = decreases {
                analyze_expr(d, fields, a);
            }
        }
        ExprKind::WhileLet { scrutinee, body, invariants, decreases, .. } => {
            analyze_expr(scrutinee, fields, a);
            analyze_block(body, fields, a);
            for inv in invariants {
                analyze_expr(inv, fields, a);
            }
            if let Some(d) = decreases {
                analyze_expr(d, fields, a);
            }
        }
        ExprKind::Loop { body, invariants, decreases } => {
            analyze_block(body, fields, a);
            for inv in invariants {
                analyze_expr(inv, fields, a);
            }
            if let Some(d) = decreases {
                analyze_expr(d, fields, a);
            }
        }
        ExprKind::Select { arms } => {
            for arm in arms {
                analyze_select_op(&arm.op, fields, a);
                if let Some(g) = &arm.guard {
                    analyze_expr(g, fields, a);
                }
                analyze_block(&arm.body, fields, a);
            }
        }
        ExprKind::Lambda { .. }
        | ExprKind::ClosureLight { .. }
        | ExprKind::ClosureFull(_)
        | ExprKind::HandlerLit { .. }
        | ExprKind::ProtocolLit { .. } => {
            // Already handled in analyze_expr (closure-capture).
        }
        ExprKind::With { bindings, body } => {
            for wb in bindings {
                analyze_expr(&wb.handler, fields, a);
            }
            analyze_block(body, fields, a);
        }
        ExprKind::Interrupt(opt) => {
            if let Some(e) = opt {
                analyze_expr(e, fields, a);
            }
        }
        ExprKind::Forbid { body, .. } => analyze_block(body, fields, a),
        ExprKind::Realtime { body, .. } => analyze_block(body, fields, a),
        ExprKind::Range { start, end, .. } => {
            if let Some(s) = start {
                analyze_expr(s, fields, a);
            }
            if let Some(e) = end {
                analyze_expr(e, fields, a);
            }
        }
        ExprKind::Forall { range, body, .. } | ExprKind::Exists { range, body, .. } => {
            analyze_expr(range, fields, a);
            analyze_expr(body, fields, a);
        }
        ExprKind::Block(b) => analyze_block(b, fields, a),
        ExprKind::Spawn(e) => analyze_expr(e, fields, a),
        ExprKind::Supervised { body, cancel } => {
            analyze_block(body, fields, a);
            if let Some(c) = cancel {
                analyze_expr(c, fields, a);
            }
        }
        ExprKind::Detach(b) | ExprKind::Blocking(b) => analyze_block(b, fields, a),
        ExprKind::Throw(e) => analyze_expr(e, fields, a),
        ExprKind::TaggedTemplate { tag, args, .. } => {
            analyze_expr(tag, fields, a);
            for arg in args {
                analyze_expr(arg, fields, a);
            }
        }
    }
}

fn analyze_else(eb: &ElseBranch, fields: &HashMap<String, FieldKind>, a: &mut FnAnalysis) {
    match eb {
        ElseBranch::Block(b) => analyze_block(b, fields, a),
        ElseBranch::If(e) => analyze_expr(e, fields, a),
    }
}

fn analyze_trailing(t: &Trailing, fields: &HashMap<String, FieldKind>, a: &mut FnAnalysis) {
    match t {
        Trailing::Block(b) => analyze_block(b, fields, a),
        Trailing::Fn(sb) => match &sb.body {
            FnBody::Expr(e) => analyze_expr(e, fields, a),
            FnBody::Block(blk) => analyze_block(blk, fields, a),
            FnBody::External => {}
        },
        Trailing::LegacyBlockWithParams(tb) => analyze_block(&tb.body, fields, a),
    }
}

fn analyze_select_op(op: &SelectOp, fields: &HashMap<String, FieldKind>, a: &mut FnAnalysis) {
    match op {
        SelectOp::Recv { chan, .. } => analyze_expr(chan, fields, a),
        SelectOp::Send { chan, value } => {
            analyze_expr(chan, fields, a);
            analyze_expr(value, fields, a);
        }
        SelectOp::Default => {}
    }
}

// ----- Closure-body capture scanner (parallel walker) -----

fn scan_block(b: &Block, fields: &HashMap<String, FieldKind>, out: &mut HashSet<String>) {
    for s in &b.stmts {
        scan_stmt(s, fields, out);
    }
    if let Some(t) = &b.trailing {
        scan_expr(t, fields, out);
    }
}

fn scan_stmt(s: &Stmt, fields: &HashMap<String, FieldKind>, out: &mut HashSet<String>) {
    match s {
        Stmt::Let(d) => scan_expr(&d.value, fields, out),
        Stmt::Const(d) => scan_expr(&d.value, fields, out),
        Stmt::Expr(e) => scan_expr(e, fields, out),
        Stmt::Assign { target, value, .. } => {
            scan_expr(target, fields, out);
            scan_expr(value, fields, out);
        }
        Stmt::Return { value, .. } => {
            if let Some(v) = value {
                scan_expr(v, fields, out);
            }
        }
        Stmt::Throw { value, .. } => scan_expr(value, fields, out),
        Stmt::Defer { body, .. } | Stmt::ErrDefer { body, .. }
        | Stmt::OkDefer { body, .. } | Stmt::DeferWithResult { body, .. } => {
            scan_expr(body, fields, out);
        }
        Stmt::ConsumeScope { init, body, .. } => {
            scan_expr(init, fields, out);
            scan_block(body, fields, out);
        }
        Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => {
            scan_expr(expr, fields, out);
        }
        Stmt::Break(_) | Stmt::Continue(_)
        | Stmt::Apply { .. } | Stmt::Calc { .. } | Stmt::Reveal { .. } => {}
    }
}

fn scan_expr(e: &Expr, fields: &HashMap<String, FieldKind>, out: &mut HashSet<String>) {
    if let Some(fname) = match_self_field(e) {
        if fields.contains_key(fname) {
            out.insert(fname.to_string());
        }
        return;
    }
    match &e.kind {
        ExprKind::IntLit(_) | ExprKind::FloatLit(_) | ExprKind::StrLit(_)
        | ExprKind::BoolLit(_) | ExprKind::UnitLit | ExprKind::CharLit(_)
        | ExprKind::NullPtrLit | ExprKind::Ident(_) | ExprKind::Path(_)
        | ExprKind::SelfAccess => {}

        ExprKind::InterpolatedStr { parts } => {
            for p in parts {
                if let InterpStrPart::Expr(e) = p {
                    scan_expr(e, fields, out);
                }
            }
        }
        ExprKind::ArrayLit(elems) => {
            for el in elems {
                match el {
                    ArrayElem::Item(e) | ArrayElem::Spread(e) => scan_expr(e, fields, out),
                }
            }
        }
        ExprKind::MapLit { elems, .. } => {
            for el in elems {
                match el {
                    MapElem::Pair(k, v) => {
                        scan_expr(k, fields, out);
                        scan_expr(v, fields, out);
                    }
                    MapElem::Spread(e) => scan_expr(e, fields, out),
                }
            }
        }
        ExprKind::RecordLit { fields: rfields, .. } => {
            for rf in rfields {
                if let Some(v) = &rf.value {
                    scan_expr(v, fields, out);
                }
            }
        }
        ExprKind::TupleLit(elems) => {
            for el in elems {
                scan_expr(el, fields, out);
            }
        }
        ExprKind::Member { obj, .. } => scan_expr(obj, fields, out),
        ExprKind::Index { obj, index } => {
            scan_expr(obj, fields, out);
            scan_expr(index, fields, out);
        }
        ExprKind::TurboFish { base, .. } => scan_expr(base, fields, out),
        ExprKind::Call { func, args, trailing } => {
            scan_expr(func, fields, out);
            for arg in args {
                match arg {
                    CallArg::Item(e) | CallArg::Spread(e) => scan_expr(e, fields, out),
                    CallArg::Named { value, .. } => scan_expr(value, fields, out),
                }
            }
            if let Some(t) = trailing {
                scan_trailing(t, fields, out);
            }
        }
        ExprKind::Try(e) | ExprKind::Bang(e) => scan_expr(e, fields, out),
        ExprKind::Coalesce(a, b) => {
            scan_expr(a, fields, out);
            scan_expr(b, fields, out);
        }
        ExprKind::As(e, _) | ExprKind::Is(e, _) => scan_expr(e, fields, out),
        ExprKind::Binary { left, right, .. } => {
            scan_expr(left, fields, out);
            scan_expr(right, fields, out);
        }
        ExprKind::Unary { operand, .. } => scan_expr(operand, fields, out),
        ExprKind::If { cond, then, else_ } => {
            scan_expr(cond, fields, out);
            scan_block(then, fields, out);
            if let Some(eb) = else_ {
                scan_else(eb, fields, out);
            }
        }
        ExprKind::IfLet { scrutinee, then, else_, .. } => {
            scan_expr(scrutinee, fields, out);
            scan_block(then, fields, out);
            if let Some(eb) = else_ {
                scan_else(eb, fields, out);
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            scan_expr(scrutinee, fields, out);
            for arm in arms {
                if let Some(g) = &arm.guard {
                    scan_expr(g, fields, out);
                }
                match &arm.body {
                    MatchArmBody::Expr(e) => scan_expr(e, fields, out),
                    MatchArmBody::Block(b) => scan_block(b, fields, out),
                }
            }
        }
        ExprKind::For { iter, body, invariants, decreases, .. } => {
            scan_expr(iter, fields, out);
            scan_block(body, fields, out);
            for inv in invariants {
                scan_expr(inv, fields, out);
            }
            if let Some(d) = decreases {
                scan_expr(d, fields, out);
            }
        }
        ExprKind::ParallelFor { iter, body, .. } => {
            scan_expr(iter, fields, out);
            scan_block(body, fields, out);
        }
        ExprKind::While { cond, body, invariants, decreases } => {
            scan_expr(cond, fields, out);
            scan_block(body, fields, out);
            for inv in invariants {
                scan_expr(inv, fields, out);
            }
            if let Some(d) = decreases {
                scan_expr(d, fields, out);
            }
        }
        ExprKind::WhileLet { scrutinee, body, invariants, decreases, .. } => {
            scan_expr(scrutinee, fields, out);
            scan_block(body, fields, out);
            for inv in invariants {
                scan_expr(inv, fields, out);
            }
            if let Some(d) = decreases {
                scan_expr(d, fields, out);
            }
        }
        ExprKind::Loop { body, invariants, decreases } => {
            scan_block(body, fields, out);
            for inv in invariants {
                scan_expr(inv, fields, out);
            }
            if let Some(d) = decreases {
                scan_expr(d, fields, out);
            }
        }
        ExprKind::Select { arms } => {
            for arm in arms {
                match &arm.op {
                    SelectOp::Recv { chan, .. } => scan_expr(chan, fields, out),
                    SelectOp::Send { chan, value } => {
                        scan_expr(chan, fields, out);
                        scan_expr(value, fields, out);
                    }
                    SelectOp::Default => {}
                }
                if let Some(g) = &arm.guard {
                    scan_expr(g, fields, out);
                }
                scan_block(&arm.body, fields, out);
            }
        }
        ExprKind::Lambda { body, .. } => scan_expr(body, fields, out),
        ExprKind::ClosureLight { body, .. } => scan_closure_body(body, fields, out),
        ExprKind::ClosureFull(b) => scan_fn_body(&b.body, fields, out),
        ExprKind::HandlerLit { methods, .. } | ExprKind::ProtocolLit { methods, .. } => {
            for m in methods {
                match &m.body {
                    HandlerMethodBody::Expr(e) => scan_expr(e, fields, out),
                    HandlerMethodBody::Block(b) => scan_block(b, fields, out),
                }
            }
        }
        ExprKind::With { bindings, body } => {
            for wb in bindings {
                scan_expr(&wb.handler, fields, out);
            }
            scan_block(body, fields, out);
        }
        ExprKind::Interrupt(opt) => {
            if let Some(e) = opt {
                scan_expr(e, fields, out);
            }
        }
        ExprKind::Forbid { body, .. } => scan_block(body, fields, out),
        ExprKind::Realtime { body, .. } => scan_block(body, fields, out),
        ExprKind::Range { start, end, .. } => {
            if let Some(s) = start {
                scan_expr(s, fields, out);
            }
            if let Some(e) = end {
                scan_expr(e, fields, out);
            }
        }
        ExprKind::Forall { range, body, .. } | ExprKind::Exists { range, body, .. } => {
            scan_expr(range, fields, out);
            scan_expr(body, fields, out);
        }
        ExprKind::Block(b) => scan_block(b, fields, out),
        ExprKind::Spawn(e) => scan_expr(e, fields, out),
        ExprKind::Supervised { body, cancel } => {
            scan_block(body, fields, out);
            if let Some(c) = cancel {
                scan_expr(c, fields, out);
            }
        }
        ExprKind::Detach(b) | ExprKind::Blocking(b) => scan_block(b, fields, out),
        ExprKind::Throw(e) => scan_expr(e, fields, out),
        ExprKind::TaggedTemplate { tag, args, .. } => {
            scan_expr(tag, fields, out);
            for arg in args {
                scan_expr(arg, fields, out);
            }
        }
    }
}

fn scan_else(eb: &ElseBranch, fields: &HashMap<String, FieldKind>, out: &mut HashSet<String>) {
    match eb {
        ElseBranch::Block(b) => scan_block(b, fields, out),
        ElseBranch::If(e) => scan_expr(e, fields, out),
    }
}

fn scan_trailing(t: &Trailing, fields: &HashMap<String, FieldKind>, out: &mut HashSet<String>) {
    match t {
        Trailing::Block(b) => scan_block(b, fields, out),
        Trailing::Fn(sb) => scan_fn_body(&sb.body, fields, out),
        Trailing::LegacyBlockWithParams(tb) => scan_block(&tb.body, fields, out),
    }
}

fn scan_closure_body(body: &ClosureBody, fields: &HashMap<String, FieldKind>, out: &mut HashSet<String>) {
    match body {
        ClosureBody::Expr(e) => scan_expr(e, fields, out),
        ClosureBody::Block(b) => scan_block(b, fields, out),
    }
}

fn scan_fn_body(body: &FnBody, fields: &HashMap<String, FieldKind>, out: &mut HashSet<String>) {
    match body {
        FnBody::Expr(e) => scan_expr(e, fields, out),
        FnBody::Block(b) => scan_block(b, fields, out),
        FnBody::External => {}
    }
}

// ----- Local name collection (for cache collision avoidance) -----

fn collect_local_names_fn(f: &FnDecl, out: &mut HashSet<String>) {
    match &f.body {
        FnBody::Block(b) => collect_locals_block(b, out),
        FnBody::Expr(e) => collect_locals_expr(e, out),
        FnBody::External => {}
    }
}

fn collect_locals_block(b: &Block, out: &mut HashSet<String>) {
    for s in &b.stmts {
        collect_locals_stmt(s, out);
    }
    if let Some(t) = &b.trailing {
        collect_locals_expr(t, out);
    }
}

fn collect_locals_stmt(s: &Stmt, out: &mut HashSet<String>) {
    match s {
        Stmt::Let(d) => {
            collect_pattern_names(&d.pattern, out);
            collect_locals_expr(&d.value, out);
        }
        Stmt::Const(d) => {
            out.insert(d.name.clone());
            collect_locals_expr(&d.value, out);
        }
        Stmt::Expr(e) => collect_locals_expr(e, out),
        Stmt::Assign { target, value, .. } => {
            collect_locals_expr(target, out);
            collect_locals_expr(value, out);
        }
        Stmt::Return { value, .. } => {
            if let Some(v) = value {
                collect_locals_expr(v, out);
            }
        }
        Stmt::Throw { value, .. } => collect_locals_expr(value, out),
        Stmt::Break(_) | Stmt::Continue(_)
        | Stmt::Apply { .. } | Stmt::Calc { .. } | Stmt::Reveal { .. } => {}
        Stmt::Defer { body, .. } | Stmt::ErrDefer { body, .. }
        | Stmt::OkDefer { body, .. } | Stmt::DeferWithResult { body, .. } => {
            collect_locals_expr(body, out);
        }
        Stmt::ConsumeScope { binding, init, body, .. } => {
            out.insert(binding.clone());
            collect_locals_expr(init, out);
            collect_locals_block(body, out);
        }
        Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => {
            collect_locals_expr(expr, out);
        }
    }
}

fn collect_pattern_names(p: &Pattern, out: &mut HashSet<String>) {
    match p {
        Pattern::Ident { name, .. } => {
            out.insert(name.clone());
        }
        Pattern::Tuple(pats, _) => {
            for sub in pats {
                collect_pattern_names(sub, out);
            }
        }
        Pattern::Record { fields, .. } => {
            for rf in fields {
                if let Some(sub) = &rf.pattern {
                    collect_pattern_names(sub, out);
                } else {
                    out.insert(rf.name.clone());
                }
            }
        }
        Pattern::Variant { kind, .. } => {
            if let VariantPatternKind::Tuple { patterns, .. } = kind {
                for sub in patterns {
                    collect_pattern_names(sub, out);
                }
            }
        }
        Pattern::Array { elems, .. } => {
            for el in elems {
                match el {
                    ArrayPatternElem::Item(p) => collect_pattern_names(p, out),
                    ArrayPatternElem::Rest => {}
                    ArrayPatternElem::RestBind(name) => {
                        out.insert(name.clone());
                    }
                }
            }
        }
        Pattern::Binding { name, inner, .. } => {
            out.insert(name.clone());
            collect_pattern_names(inner, out);
        }
        Pattern::Or { alternatives, .. } => {
            // Or-patterns share bindings — take from first alt.
            if let Some(first) = alternatives.first() {
                collect_pattern_names(first, out);
            }
        }
        Pattern::Wildcard(_) | Pattern::Literal(_, _) => {}
    }
}

fn collect_locals_expr(e: &Expr, out: &mut HashSet<String>) {
    match &e.kind {
        ExprKind::Block(b) => collect_locals_block(b, out),
        ExprKind::If { cond, then, else_ } => {
            collect_locals_expr(cond, out);
            collect_locals_block(then, out);
            if let Some(eb) = else_ {
                match eb {
                    ElseBranch::Block(b) => collect_locals_block(b, out),
                    ElseBranch::If(e) => collect_locals_expr(e, out),
                }
            }
        }
        ExprKind::IfLet { pattern, scrutinee, then, else_, .. } => {
            collect_pattern_names(pattern, out);
            collect_locals_expr(scrutinee, out);
            collect_locals_block(then, out);
            if let Some(eb) = else_ {
                match eb {
                    ElseBranch::Block(b) => collect_locals_block(b, out),
                    ElseBranch::If(e) => collect_locals_expr(e, out),
                }
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            collect_locals_expr(scrutinee, out);
            for arm in arms {
                collect_pattern_names(&arm.pattern, out);
                if let Some(g) = &arm.guard {
                    collect_locals_expr(g, out);
                }
                match &arm.body {
                    MatchArmBody::Expr(e) => collect_locals_expr(e, out),
                    MatchArmBody::Block(b) => collect_locals_block(b, out),
                }
            }
        }
        ExprKind::For { pattern, iter, body, .. }
        | ExprKind::ParallelFor { pattern, iter, body, .. } => {
            collect_pattern_names(pattern, out);
            collect_locals_expr(iter, out);
            collect_locals_block(body, out);
        }
        ExprKind::While { cond, body, .. } => {
            collect_locals_expr(cond, out);
            collect_locals_block(body, out);
        }
        ExprKind::WhileLet { pattern, scrutinee, body, .. } => {
            collect_pattern_names(pattern, out);
            collect_locals_expr(scrutinee, out);
            collect_locals_block(body, out);
        }
        ExprKind::Loop { body, .. } => collect_locals_block(body, out),
        _ => walk_children_for_locals(e, out),
    }
}

fn walk_children_for_locals(e: &Expr, out: &mut HashSet<String>) {
    match &e.kind {
        ExprKind::IntLit(_) | ExprKind::FloatLit(_) | ExprKind::StrLit(_)
        | ExprKind::BoolLit(_) | ExprKind::UnitLit | ExprKind::CharLit(_)
        | ExprKind::NullPtrLit | ExprKind::Ident(_) | ExprKind::Path(_)
        | ExprKind::SelfAccess => {}
        ExprKind::InterpolatedStr { parts } => {
            for p in parts {
                if let InterpStrPart::Expr(e) = p {
                    collect_locals_expr(e, out);
                }
            }
        }
        ExprKind::ArrayLit(elems) => {
            for el in elems {
                match el {
                    ArrayElem::Item(e) | ArrayElem::Spread(e) => collect_locals_expr(e, out),
                }
            }
        }
        ExprKind::MapLit { elems, .. } => {
            for el in elems {
                match el {
                    MapElem::Pair(k, v) => {
                        collect_locals_expr(k, out);
                        collect_locals_expr(v, out);
                    }
                    MapElem::Spread(e) => collect_locals_expr(e, out),
                }
            }
        }
        ExprKind::RecordLit { fields, .. } => {
            for rf in fields {
                if let Some(v) = &rf.value {
                    collect_locals_expr(v, out);
                }
            }
        }
        ExprKind::TupleLit(elems) => {
            for el in elems {
                collect_locals_expr(el, out);
            }
        }
        ExprKind::Member { obj, .. } => collect_locals_expr(obj, out),
        ExprKind::Index { obj, index } => {
            collect_locals_expr(obj, out);
            collect_locals_expr(index, out);
        }
        ExprKind::TurboFish { base, .. } => collect_locals_expr(base, out),
        ExprKind::Call { func, args, trailing } => {
            collect_locals_expr(func, out);
            for arg in args {
                match arg {
                    CallArg::Item(e) | CallArg::Spread(e) => collect_locals_expr(e, out),
                    CallArg::Named { value, .. } => collect_locals_expr(value, out),
                }
            }
            if let Some(t) = trailing {
                match t {
                    Trailing::Block(b) => collect_locals_block(b, out),
                    Trailing::Fn(sb) => match &sb.body {
                        FnBody::Block(b) => collect_locals_block(b, out),
                        FnBody::Expr(e) => collect_locals_expr(e, out),
                        FnBody::External => {}
                    },
                    Trailing::LegacyBlockWithParams(tb) => collect_locals_block(&tb.body, out),
                }
            }
        }
        ExprKind::Try(e) | ExprKind::Bang(e) => collect_locals_expr(e, out),
        ExprKind::Coalesce(a, b) => {
            collect_locals_expr(a, out);
            collect_locals_expr(b, out);
        }
        ExprKind::As(e, _) | ExprKind::Is(e, _) => collect_locals_expr(e, out),
        ExprKind::Binary { left, right, .. } => {
            collect_locals_expr(left, out);
            collect_locals_expr(right, out);
        }
        ExprKind::Unary { operand, .. } => collect_locals_expr(operand, out),
        ExprKind::Select { arms } => {
            for arm in arms {
                match &arm.op {
                    SelectOp::Recv { chan, binding, .. } => {
                        if let Some(b) = binding {
                            out.insert(b.clone());
                        }
                        collect_locals_expr(chan, out);
                    }
                    SelectOp::Send { chan, value } => {
                        collect_locals_expr(chan, out);
                        collect_locals_expr(value, out);
                    }
                    SelectOp::Default => {}
                }
                collect_locals_block(&arm.body, out);
            }
        }
        ExprKind::Lambda { params, body, .. } => {
            for p in params {
                out.insert(p.name.clone());
            }
            collect_locals_expr(body, out);
        }
        ExprKind::ClosureLight { params, body } => {
            for p in params {
                out.insert(p.name.clone());
            }
            match body {
                ClosureBody::Expr(e) => collect_locals_expr(e, out),
                ClosureBody::Block(b) => collect_locals_block(b, out),
            }
        }
        ExprKind::ClosureFull(b) => {
            for p in &b.params {
                out.insert(p.name.clone());
            }
            match &b.body {
                FnBody::Expr(e) => collect_locals_expr(e, out),
                FnBody::Block(blk) => collect_locals_block(blk, out),
                FnBody::External => {}
            }
        }
        ExprKind::HandlerLit { methods, .. } | ExprKind::ProtocolLit { methods, .. } => {
            for m in methods {
                for p in &m.params {
                    out.insert(p.name.clone());
                }
                match &m.body {
                    HandlerMethodBody::Expr(e) => collect_locals_expr(e, out),
                    HandlerMethodBody::Block(b) => collect_locals_block(b, out),
                }
            }
        }
        ExprKind::With { bindings, body } => {
            for wb in bindings {
                collect_locals_expr(&wb.handler, out);
            }
            collect_locals_block(body, out);
        }
        ExprKind::Interrupt(opt) => {
            if let Some(e) = opt {
                collect_locals_expr(e, out);
            }
        }
        ExprKind::Forbid { body, .. } => collect_locals_block(body, out),
        ExprKind::Realtime { body, .. } => collect_locals_block(body, out),
        ExprKind::Range { start, end, .. } => {
            if let Some(s) = start {
                collect_locals_expr(s, out);
            }
            if let Some(e) = end {
                collect_locals_expr(e, out);
            }
        }
        ExprKind::Forall { var, range, body } | ExprKind::Exists { var, range, body } => {
            out.insert(var.clone());
            collect_locals_expr(range, out);
            collect_locals_expr(body, out);
        }
        ExprKind::Spawn(e) => collect_locals_expr(e, out),
        ExprKind::Supervised { body, cancel } => {
            collect_locals_block(body, out);
            if let Some(c) = cancel {
                collect_locals_expr(c, out);
            }
        }
        ExprKind::Detach(b) | ExprKind::Blocking(b) => collect_locals_block(b, out),
        ExprKind::Throw(e) => collect_locals_expr(e, out),
        ExprKind::TaggedTemplate { tag, args, .. } => {
            collect_locals_expr(tag, out);
            for arg in args {
                collect_locals_expr(arg, out);
            }
        }
        // Block/If/IfLet/Match/For/ParallelFor/While/WhileLet/Loop —
        // обработаны вверху, не должны попадать сюда.
        _ => {}
    }
}

// ----- REWRITE phase -----

/// Объединённый rewrite: ro-fields → full-body replace; mut-fields →
/// first-region replace (до first write OR first call boundary).
fn rewrite_fn_body_split(
    f: &mut FnDecl,
    ro_cache: &[(String, crate::diag::Span)],
    mut_cache: &[(String, crate::diag::Span)],
    name_map: &HashMap<String, String>,
) {
    // Build replace maps.
    let ro_map: HashMap<String, String> = ro_cache.iter()
        .map(|(fname, _)| (fname.clone(), name_map[fname].clone()))
        .collect();
    let mut_map: HashMap<String, String> = mut_cache.iter()
        .map(|(fname, _)| (fname.clone(), name_map[fname].clone()))
        .collect();
    // Combined для prefix-let injection.
    let mut combined_cache: Vec<(String, crate::diag::Span)> = Vec::new();
    combined_cache.extend(ro_cache.iter().cloned());
    combined_cache.extend(mut_cache.iter().cloned());

    // Ensure body is Block (coerce Expr → Block-with-trailing).
    match &mut f.body {
        FnBody::Block(_) => {}
        FnBody::Expr(_) => {
            let body_expr = match std::mem::replace(&mut f.body, FnBody::External) {
                FnBody::Expr(e) => e,
                _ => unreachable!(),
            };
            let span = body_expr.span;
            let new_block = Block {
                stmts: Vec::new(),
                trailing: Some(Box::new(body_expr)),
                span,
            };
            f.body = FnBody::Block(new_block);
        }
        FnBody::External => return,
    }

    // Mut-cache rewrite — bounded к first-region. Done BEFORE prefix
    // insertion (indices stable). For each mut field, find boundary
    // в top-level stmts; replace reads only в stmts[0..boundary]
    // (включая trailing если no barrier).
    if let FnBody::Block(b) = &mut f.body {
        for (fname, _) in mut_cache {
            // Find first-barrier index в TOP-LEVEL stmts (not nested).
            let mut barrier_idx = b.stmts.len();
            for (i, s) in b.stmts.iter().enumerate() {
                if stmt_is_barrier_for(s, fname) {
                    barrier_idx = i;
                    break;
                }
            }
            // Build per-field single-entry map.
            let single: HashMap<String, String> = std::iter::once((fname.clone(), mut_map[fname].clone())).collect();
            // Rewrite stmts[0..barrier_idx].
            for s in b.stmts.iter_mut().take(barrier_idx) {
                rewrite_stmt(s, &single);
            }
            // If no barrier — also rewrite trailing.
            if barrier_idx == b.stmts.len() {
                // Trailing еще не обработан barrier — но мог сам быть
                // barrier'ом. Проверим.
                if let Some(t) = &mut b.trailing {
                    let trailing_barrier = expr_is_barrier_for(t, fname);
                    if !trailing_barrier {
                        rewrite_expr(t, &single);
                    }
                }
            }
        }
    }

    // Ro-cache rewrite — full body.
    if !ro_map.is_empty() {
        if let FnBody::Block(b) = &mut f.body {
            rewrite_block(b, &ro_map);
        }
    }

    // Prepend cache lets. Order: ro first, then mut (sorted внутри
    // через name_map keys insertion order, но мы передаём через
    // combined_cache).
    if let FnBody::Block(b) = &mut f.body {
        let mut prefix_stmts: Vec<Stmt> = Vec::with_capacity(combined_cache.len());
        for (fname, span) in &combined_cache {
            let local_name = &name_map[fname];
            let access = Expr {
                kind: ExprKind::Member {
                    obj: Box::new(Expr { kind: ExprKind::SelfAccess, span: *span }),
                    name: fname.clone(),
                },
                span: *span,
            };
            let let_stmt = Stmt::Let(LetDecl {
                mutable: false,
                pattern: Pattern::Ident {
                    name: local_name.clone(),
                    span: *span,
                    is_mut: false,
                },
                ty: None,
                value: access,
                span: *span,
                is_ghost: false,
                consume: false,
            });
            prefix_stmts.push(let_stmt);
        }
        prefix_stmts.append(&mut b.stmts);
        b.stmts = prefix_stmts;
    }
}

fn rewrite_block(b: &mut Block, replace_map: &HashMap<String, String>) {
    for s in &mut b.stmts {
        rewrite_stmt(s, replace_map);
    }
    if let Some(t) = &mut b.trailing {
        rewrite_expr(t, replace_map);
    }
}

fn rewrite_stmt(s: &mut Stmt, replace_map: &HashMap<String, String>) {
    match s {
        Stmt::Let(d) => rewrite_expr(&mut d.value, replace_map),
        Stmt::Const(d) => rewrite_expr(&mut d.value, replace_map),
        Stmt::Expr(e) => rewrite_expr(e, replace_map),
        Stmt::Assign { target, value, .. } => {
            // For top-level `@F = ...` — DO NOT rewrite target (it's a
            // write target, must remain field-write). Ro fields can't
            // be assignment targets (type checker enforces), so for
            // Ф.1 the relevant fields here are mut (not in replace_map
            // anyway). For complex LHS like `@F[i]` — `@F` would be a
            // *read* (for indexing), but we skip rewrite to be safe
            // в Ф.1 (Ф.2 handles).
            if match_self_field(target).is_none() {
                rewrite_expr(target, replace_map);
            }
            rewrite_expr(value, replace_map);
        }
        Stmt::Return { value, .. } => {
            if let Some(v) = value {
                rewrite_expr(v, replace_map);
            }
        }
        Stmt::Throw { value, .. } => rewrite_expr(value, replace_map),
        Stmt::Break(_) | Stmt::Continue(_)
        | Stmt::Apply { .. } | Stmt::Calc { .. } | Stmt::Reveal { .. } => {}
        Stmt::Defer { body, .. } | Stmt::ErrDefer { body, .. }
        | Stmt::OkDefer { body, .. } | Stmt::DeferWithResult { body, .. } => {
            rewrite_expr(body, replace_map);
        }
        Stmt::ConsumeScope { init, body, .. } => {
            rewrite_expr(init, replace_map);
            rewrite_block(body, replace_map);
        }
        Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => {
            rewrite_expr(expr, replace_map);
        }
    }
}

fn rewrite_expr(e: &mut Expr, replace_map: &HashMap<String, String>) {
    // Top-level `@F` match → replace.
    if let ExprKind::Member { obj, name } = &e.kind {
        if matches!(obj.kind, ExprKind::SelfAccess) {
            if let Some(local) = replace_map.get(name) {
                e.kind = ExprKind::Ident(local.clone());
                return;
            }
        }
    }

    // Don't recurse into closure bodies (they're in different scope —
    // cache local doesn't exist there; semantic preservation requires
    // direct field access внутри closure).
    match &e.kind {
        ExprKind::ClosureLight { .. } | ExprKind::ClosureFull(_)
        | ExprKind::Lambda { .. } | ExprKind::HandlerLit { .. }
        | ExprKind::ProtocolLit { .. } => return,
        _ => {}
    }

    rewrite_expr_children(e, replace_map);
}

fn rewrite_expr_children(e: &mut Expr, replace_map: &HashMap<String, String>) {
    match &mut e.kind {
        ExprKind::IntLit(_) | ExprKind::FloatLit(_) | ExprKind::StrLit(_)
        | ExprKind::BoolLit(_) | ExprKind::UnitLit | ExprKind::CharLit(_)
        | ExprKind::NullPtrLit | ExprKind::Ident(_) | ExprKind::Path(_)
        | ExprKind::SelfAccess => {}

        ExprKind::InterpolatedStr { parts } => {
            for p in parts {
                if let InterpStrPart::Expr(e) = p {
                    rewrite_expr(e, replace_map);
                }
            }
        }
        ExprKind::ArrayLit(elems) => {
            for el in elems {
                match el {
                    ArrayElem::Item(e) | ArrayElem::Spread(e) => rewrite_expr(e, replace_map),
                }
            }
        }
        ExprKind::MapLit { elems, .. } => {
            for el in elems {
                match el {
                    MapElem::Pair(k, v) => {
                        rewrite_expr(k, replace_map);
                        rewrite_expr(v, replace_map);
                    }
                    MapElem::Spread(e) => rewrite_expr(e, replace_map),
                }
            }
        }
        ExprKind::RecordLit { fields, .. } => {
            for rf in fields {
                if let Some(v) = &mut rf.value {
                    rewrite_expr(v, replace_map);
                }
            }
        }
        ExprKind::TupleLit(elems) => {
            for el in elems {
                rewrite_expr(el, replace_map);
            }
        }
        ExprKind::Member { obj, .. } => rewrite_expr(obj, replace_map),
        ExprKind::Index { obj, index } => {
            rewrite_expr(obj, replace_map);
            rewrite_expr(index, replace_map);
        }
        ExprKind::TurboFish { base, .. } => rewrite_expr(base, replace_map),
        ExprKind::Call { func, args, trailing } => {
            rewrite_expr(func, replace_map);
            for arg in args {
                match arg {
                    CallArg::Item(e) | CallArg::Spread(e) => rewrite_expr(e, replace_map),
                    CallArg::Named { value, .. } => rewrite_expr(value, replace_map),
                }
            }
            if let Some(t) = trailing {
                rewrite_trailing(t, replace_map);
            }
        }
        ExprKind::Try(e) | ExprKind::Bang(e) => rewrite_expr(e, replace_map),
        ExprKind::Coalesce(a, b) => {
            rewrite_expr(a, replace_map);
            rewrite_expr(b, replace_map);
        }
        ExprKind::As(e, _) | ExprKind::Is(e, _) => rewrite_expr(e, replace_map),
        ExprKind::Binary { left, right, .. } => {
            rewrite_expr(left, replace_map);
            rewrite_expr(right, replace_map);
        }
        ExprKind::Unary { operand, .. } => rewrite_expr(operand, replace_map),
        ExprKind::If { cond, then, else_ } => {
            rewrite_expr(cond, replace_map);
            rewrite_block(then, replace_map);
            if let Some(eb) = else_ {
                rewrite_else(eb, replace_map);
            }
        }
        ExprKind::IfLet { scrutinee, then, else_, .. } => {
            rewrite_expr(scrutinee, replace_map);
            rewrite_block(then, replace_map);
            if let Some(eb) = else_ {
                rewrite_else(eb, replace_map);
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            rewrite_expr(scrutinee, replace_map);
            for arm in arms {
                if let Some(g) = &mut arm.guard {
                    rewrite_expr(g, replace_map);
                }
                match &mut arm.body {
                    MatchArmBody::Expr(e) => rewrite_expr(e, replace_map),
                    MatchArmBody::Block(b) => rewrite_block(b, replace_map),
                }
            }
        }
        ExprKind::For { iter, body, invariants, decreases, .. } => {
            rewrite_expr(iter, replace_map);
            rewrite_block(body, replace_map);
            for inv in invariants {
                rewrite_expr(inv, replace_map);
            }
            if let Some(d) = decreases {
                rewrite_expr(d, replace_map);
            }
        }
        ExprKind::ParallelFor { iter, body, .. } => {
            rewrite_expr(iter, replace_map);
            rewrite_block(body, replace_map);
        }
        ExprKind::While { cond, body, invariants, decreases } => {
            rewrite_expr(cond, replace_map);
            rewrite_block(body, replace_map);
            for inv in invariants {
                rewrite_expr(inv, replace_map);
            }
            if let Some(d) = decreases {
                rewrite_expr(d, replace_map);
            }
        }
        ExprKind::WhileLet { scrutinee, body, invariants, decreases, .. } => {
            rewrite_expr(scrutinee, replace_map);
            rewrite_block(body, replace_map);
            for inv in invariants {
                rewrite_expr(inv, replace_map);
            }
            if let Some(d) = decreases {
                rewrite_expr(d, replace_map);
            }
        }
        ExprKind::Loop { body, invariants, decreases } => {
            rewrite_block(body, replace_map);
            for inv in invariants {
                rewrite_expr(inv, replace_map);
            }
            if let Some(d) = decreases {
                rewrite_expr(d, replace_map);
            }
        }
        ExprKind::Select { arms } => {
            for arm in arms {
                match &mut arm.op {
                    SelectOp::Recv { chan, .. } => rewrite_expr(chan, replace_map),
                    SelectOp::Send { chan, value } => {
                        rewrite_expr(chan, replace_map);
                        rewrite_expr(value, replace_map);
                    }
                    SelectOp::Default => {}
                }
                if let Some(g) = &mut arm.guard {
                    rewrite_expr(g, replace_map);
                }
                rewrite_block(&mut arm.body, replace_map);
            }
        }
        ExprKind::Lambda { .. } | ExprKind::ClosureLight { .. }
        | ExprKind::ClosureFull(_) | ExprKind::HandlerLit { .. }
        | ExprKind::ProtocolLit { .. } => {
            // Handled by early-return в rewrite_expr.
        }
        ExprKind::With { bindings, body } => {
            for wb in bindings {
                rewrite_expr(&mut wb.handler, replace_map);
            }
            rewrite_block(body, replace_map);
        }
        ExprKind::Interrupt(opt) => {
            if let Some(e) = opt {
                rewrite_expr(e, replace_map);
            }
        }
        ExprKind::Forbid { body, .. } => rewrite_block(body, replace_map),
        ExprKind::Realtime { body, .. } => rewrite_block(body, replace_map),
        ExprKind::Range { start, end, .. } => {
            if let Some(s) = start {
                rewrite_expr(s, replace_map);
            }
            if let Some(e) = end {
                rewrite_expr(e, replace_map);
            }
        }
        ExprKind::Forall { range, body, .. } | ExprKind::Exists { range, body, .. } => {
            rewrite_expr(range, replace_map);
            rewrite_expr(body, replace_map);
        }
        ExprKind::Block(b) => rewrite_block(b, replace_map),
        ExprKind::Spawn(e) => rewrite_expr(e, replace_map),
        ExprKind::Supervised { body, cancel } => {
            rewrite_block(body, replace_map);
            if let Some(c) = cancel {
                rewrite_expr(c, replace_map);
            }
        }
        ExprKind::Detach(b) | ExprKind::Blocking(b) => rewrite_block(b, replace_map),
        ExprKind::Throw(e) => rewrite_expr(e, replace_map),
        ExprKind::TaggedTemplate { tag, args, .. } => {
            rewrite_expr(tag, replace_map);
            for arg in args {
                rewrite_expr(arg, replace_map);
            }
        }
    }
}

fn rewrite_else(eb: &mut ElseBranch, replace_map: &HashMap<String, String>) {
    match eb {
        ElseBranch::Block(b) => rewrite_block(b, replace_map),
        ElseBranch::If(e) => rewrite_expr(e, replace_map),
    }
}

fn rewrite_trailing(t: &mut Trailing, replace_map: &HashMap<String, String>) {
    match t {
        Trailing::Block(b) => rewrite_block(b, replace_map),
        Trailing::Fn(_) | Trailing::LegacyBlockWithParams(_) => {
            // Trailing closure scopes — same logic as ClosureLight/Full
            // (different scope, no cache local). Skip recursion.
        }
    }
}
