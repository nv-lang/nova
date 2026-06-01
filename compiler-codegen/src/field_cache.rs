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
    /// (Ф.2 path). Ф.1 collected but not emitted.
    #[allow(dead_code)]
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

    // Decide which fields to cache (Ф.1: ro only).
    let mut field_names: Vec<&String> = analysis.read_counts.keys().collect();
    field_names.sort();
    let mut to_cache: Vec<(String, FieldKind, crate::diag::Span)> = Vec::new();
    for fname in field_names {
        let count = analysis.read_counts.get(fname).copied().unwrap_or(0);
        if count < cfg.threshold {
            continue;
        }
        if analysis.closure_captured.contains(fname) {
            continue;
        }
        let kind = match fields.get(fname) {
            Some(k) => *k,
            None => continue,
        };
        // Ф.1: ro path only.
        if !matches!(kind, FieldKind::Ro) {
            continue;
        }
        let span = analysis.first_span.get(fname).copied().unwrap_or(body_span);
        to_cache.push((fname.clone(), kind, span));
        if to_cache.len() >= cfg.max_per_fn {
            break;
        }
    }

    if to_cache.is_empty() {
        return;
    }

    // Generate cache local names (collision-avoidance).
    let mut name_map: HashMap<String, String> = HashMap::new();
    for (fname, _kind, _) in &to_cache {
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

    rewrite_fn_body(f, &to_cache, &name_map);
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

// ----- REWRITE phase (Ф.1 — ro-fields only) -----

fn rewrite_fn_body(
    f: &mut FnDecl,
    to_cache: &[(String, FieldKind, crate::diag::Span)],
    name_map: &HashMap<String, String>,
) {
    let replace_map: HashMap<String, String> = to_cache.iter()
        .map(|(fname, _kind, _)| (fname.clone(), name_map[fname].clone()))
        .collect();

    // Convert Expr-body → Block-body, чтобы prepend prefix lets.
    match &mut f.body {
        FnBody::Block(b) => {
            rewrite_block(b, &replace_map);
        }
        FnBody::Expr(_) => {
            let body_expr = match std::mem::replace(&mut f.body, FnBody::External) {
                FnBody::Expr(e) => e,
                _ => unreachable!(),
            };
            let span = body_expr.span;
            let mut new_block = Block {
                stmts: Vec::new(),
                trailing: Some(Box::new(body_expr)),
                span,
            };
            if let Some(t) = &mut new_block.trailing {
                rewrite_expr(t, &replace_map);
            }
            f.body = FnBody::Block(new_block);
        }
        FnBody::External => return,
    }

    if let FnBody::Block(b) = &mut f.body {
        let mut prefix_stmts: Vec<Stmt> = Vec::with_capacity(to_cache.len());
        for (fname, _kind, span) in to_cache {
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
