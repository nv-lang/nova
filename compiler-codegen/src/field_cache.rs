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
    /// Plan 123.2 (D218): enable Loop-Invariant Code Motion phase.
    /// LICM phase runs BEFORE per-fn ro/mut caching и hoist'ит
    /// invariant `@<F>` reads из loop bodies в pre-loop position.
    pub licm_enabled: bool,
    /// Plan 123.2 LICM threshold: min reads inside a single loop body
    /// to trigger hoist. Default 2.
    pub licm_threshold: usize,
    /// Plan 123.2 LICM cap: max hoists per loop. Default 4.
    pub licm_max_per_loop: usize,
    /// Plan 123.3 (D219): enable pure-call result caching. Caches
    /// `@<pure_method>()` results when method is Purity::Pure и
    /// no @F mutation в body.
    pub pure_enabled: bool,
    /// Plan 123.3 threshold: min pure-call occurrences. Default 2.
    pub pure_threshold: usize,
    /// Plan 123.4 (D217 amend): enable chain caching `@a.b.c`.
    pub chain_enabled: bool,
    /// Plan 123.4 threshold: min chain occurrences. Default 2.
    pub chain_threshold: usize,
    /// Plan 123.4 max chain depth (avoid stack bloat). Default 4.
    pub chain_max_depth: usize,
    /// Plan 123.7 (D223): enable inter-procedural analysis для refined
    /// mut field cache invalidation. Computes per-method field_write_set
    /// и allows caller mut-cache survive calls to non-mutating methods.
    pub ipa_enabled: bool,
    /// Plan 123.6.3 (V6.3, 2026-06-02): IPA iterative-closure iteration
    /// cap. Default 10 — covers all realistic call graphs (mutual
    /// recursion depth ≪ 10 for typical Nova modules). Configurable
    /// via `NOVA_FIELD_CACHE_IPA_ITER` env / `--field-cache-ipa-iter`
    /// CLI flag. Plan 123.7.3 SCC will supersede this with O(V+E)
    /// exact closure, but env override remains for forensics.
    pub ipa_iter_limit: usize,
}

impl Default for FieldCacheConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            threshold: 2,
            max_per_fn: 8,
            licm_enabled: true,
            licm_threshold: 2,
            licm_max_per_loop: 4,
            pure_enabled: true,
            pure_threshold: 2,
            chain_enabled: true,
            chain_threshold: 2,
            chain_max_depth: 4,
            ipa_enabled: true,
            ipa_iter_limit: 10,
        }
    }
}

impl FieldCacheConfig {
    /// `threshold == 0` → disabled (escape hatch).
    pub fn from_threshold(threshold: usize, max_per_fn: usize) -> Self {
        let base = Self::default();
        if threshold == 0 {
            Self { enabled: false, threshold: 2, max_per_fn, ..base }
        } else {
            Self { enabled: true, threshold, max_per_fn, ..base }
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
        // Plan 123.2 LICM env vars.
        if let Ok(v) = std::env::var("NOVA_FIELD_CACHE_LICM") {
            if v == "0" || v.eq_ignore_ascii_case("off") || v.eq_ignore_ascii_case("false") {
                cfg.licm_enabled = false;
            }
        }
        if let Ok(v) = std::env::var("NOVA_FIELD_CACHE_LICM_THRESHOLD") {
            if let Ok(n) = v.parse::<usize>() {
                if n == 0 {
                    cfg.licm_enabled = false;
                } else {
                    cfg.licm_threshold = n;
                }
            }
        }
        if let Ok(v) = std::env::var("NOVA_FIELD_CACHE_LICM_MAX") {
            if let Ok(n) = v.parse::<usize>() {
                if n > 0 {
                    cfg.licm_max_per_loop = n;
                }
            }
        }
        // Plan 123.3 pure-call env vars.
        if let Ok(v) = std::env::var("NOVA_FIELD_CACHE_PURE") {
            if v == "0" || v.eq_ignore_ascii_case("off") || v.eq_ignore_ascii_case("false") {
                cfg.pure_enabled = false;
            }
        }
        if let Ok(v) = std::env::var("NOVA_FIELD_CACHE_PURE_THRESHOLD") {
            if let Ok(n) = v.parse::<usize>() {
                if n == 0 {
                    cfg.pure_enabled = false;
                } else {
                    cfg.pure_threshold = n;
                }
            }
        }
        // Plan 123.4 chain env vars.
        if let Ok(v) = std::env::var("NOVA_FIELD_CACHE_CHAIN") {
            if v == "0" || v.eq_ignore_ascii_case("off") || v.eq_ignore_ascii_case("false") {
                cfg.chain_enabled = false;
            }
        }
        if let Ok(v) = std::env::var("NOVA_FIELD_CACHE_CHAIN_THRESHOLD") {
            if let Ok(n) = v.parse::<usize>() {
                if n == 0 {
                    cfg.chain_enabled = false;
                } else {
                    cfg.chain_threshold = n;
                }
            }
        }
        if let Ok(v) = std::env::var("NOVA_FIELD_CACHE_CHAIN_DEPTH") {
            if let Ok(n) = v.parse::<usize>() {
                if n >= 2 {
                    cfg.chain_max_depth = n;
                }
            }
        }
        // Plan 123.7 IPA env var.
        if let Ok(v) = std::env::var("NOVA_FIELD_CACHE_IPA") {
            if v == "0" || v.eq_ignore_ascii_case("off") || v.eq_ignore_ascii_case("false") {
                cfg.ipa_enabled = false;
            }
        }
        // Plan 123.6.3 (V6.3) — IPA iterative-closure iteration cap.
        // Clamped to [1, 1024]. `0` is rejected (would skip closure
        // entirely and silently degrade transitive write propagation).
        if let Ok(v) = std::env::var("NOVA_FIELD_CACHE_IPA_ITER") {
            if let Ok(n) = v.parse::<usize>() {
                if (1..=1024).contains(&n) {
                    cfg.ipa_iter_limit = n;
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

/// Plan 123.5 (V5): per-fn cache decision report (analyze-only,
/// no mutation). Used by `--explain-cache` CLI flag и LSP code-lens.
#[derive(Debug, Clone, Default)]
pub struct ExplainReport {
    pub per_fn: Vec<FnCacheInfo>,
}

#[derive(Debug, Clone)]
pub struct FnCacheInfo {
    pub type_name: String,
    pub fn_name: String,
    pub span: crate::diag::Span,
    /// D217 V1 ro fields decided for caching.
    pub ro_caches: Vec<String>,
    /// D217 V1 mut fields (first-region cache).
    pub mut_caches: Vec<String>,
    /// D218 LICM hoists по полю (per loop counted once per field).
    pub licm_hoists: Vec<String>,
    /// D219 pure-call cached methods.
    pub pure_caches: Vec<String>,
    /// D217 V4 chain-cached paths (each Vec<String> is path components).
    pub chain_caches: Vec<Vec<String>>,
}

impl FnCacheInfo {
    pub fn total(&self) -> usize {
        self.ro_caches.len()
            + self.mut_caches.len()
            + self.licm_hoists.len()
            + self.pure_caches.len()
            + self.chain_caches.len()
    }
}

/// Plan 123.5: analyze module без mutation. Returns ExplainReport
/// describing what caches would be inserted per fn under given config.
///
/// Clones AST internally so original module untouched. Used by:
/// - `nova check --explain-cache <file>` CLI flag.
/// - `nova-lsp` code-lens provider.
pub fn analyze_module(module: &Module, cfg: &FieldCacheConfig) -> ExplainReport {
    if !cfg.enabled {
        return ExplainReport::default();
    }
    // Clone и run cache_module on the copy; then walk the modified
    // copy to extract injected `_at_*` let statements.
    let mut module_copy = module.clone();
    cache_module(&mut module_copy, cfg);

    let mut report = ExplainReport::default();
    collect_fn_caches(&module_copy.items, &mut report);
    for pf in &module_copy.peer_files {
        collect_fn_caches(&pf.items_here, &mut report);
    }
    report
}

fn collect_fn_caches(items: &[Item], report: &mut ExplainReport) {
    for item in items {
        if let Item::Fn(f) = item {
            if let Some(recv) = &f.receiver {
                if let FnBody::Block(b) = &f.body {
                    let info = analyze_fn_for_explain(f, recv, b);
                    if info.total() > 0 {
                        report.per_fn.push(info);
                    }
                }
            }
        }
    }
}

fn analyze_fn_for_explain(f: &FnDecl, recv: &Receiver, b: &Block) -> FnCacheInfo {
    let mut info = FnCacheInfo {
        type_name: recv.type_name.clone(),
        fn_name: f.name.clone(),
        span: f.span,
        ro_caches: Vec::new(),
        mut_caches: Vec::new(),
        licm_hoists: Vec::new(),
        pure_caches: Vec::new(),
        chain_caches: Vec::new(),
    };
    // Walk prefix `let _at_*` injected statements; classify by suffix.
    for s in &b.stmts {
        if let Stmt::Let(d) = s {
            if let Pattern::Ident { name, .. } = &d.pattern {
                if !name.starts_with("_at_") {
                    break; // End of prefix lets.
                }
                // Classify by name suffix + binding shape.
                if name.ends_with("_chain") {
                    // Extract path components: _at_<a>_<b>_..._<n>_chain.
                    let inner = &name[4..name.len() - 6]; // strip "_at_" + "_chain"
                    let path: Vec<String> = inner.split('_').map(|s| s.to_string()).collect();
                    info.chain_caches.push(path);
                } else if name.ends_with("_loop") {
                    let inner = &name[4..name.len() - 5];
                    info.licm_hoists.push(inner.to_string());
                } else if name.ends_with("_call") {
                    let inner = &name[4..name.len() - 5];
                    info.pure_caches.push(inner.to_string());
                } else {
                    // _at_<field> — D217 V1 cache. Classify ro vs mut
                    // через looking up в module's TypeDecl. Здесь
                    // упрощённо — both go to ro (V1 caches both ro
                    // and mut at body prefix).
                    let fname = &name[4..];
                    // Conservative classification: check if binding's
                    // value is direct @F.
                    if let ExprKind::Member { obj, name: orig_field } = &d.value.kind {
                        if matches!(obj.kind, ExprKind::SelfAccess) {
                            // For simplicity, classify as ro (most
                            // common). Distinguishing ro vs mut requires
                            // walking TypeDecl which is available but
                            // not needed для current report shape.
                            info.ro_caches.push(orig_field.clone());
                            let _ = fname;
                        }
                    }
                }
            } else {
                break;
            }
        } else {
            break;
        }
    }
    info
}

/// Public entry-point.
pub fn cache_module(module: &mut Module, cfg: &FieldCacheConfig) {
    if !cfg.enabled {
        return;
    }
    let registry = build_registry(module);
    let pure_methods: HashSet<(String, String)> = if cfg.pure_enabled {
        build_pure_methods_registry(module)
    } else {
        HashSet::new()
    };
    // Plan 123.7 (D223): build per-method field-write-set registry для
    // IPA refinement. Maps (type_name, method_name) → set of field
    // names that method's body writes (top-level @F = ... assignments).
    let write_sets: HashMap<(String, String), HashSet<String>> = if cfg.ipa_enabled {
        build_write_set_registry(module, cfg.ipa_iter_limit)
    } else {
        HashMap::new()
    };
    // Plan 123.7.1: build per-method field-read-set registry для
    // V3.1+ frame-based pure-cache invalidation. Parallel infrastructure
    // к write_sets — direct reads + transitive closure через method calls.
    let read_sets: HashMap<(String, String), HashSet<String>> = if cfg.ipa_enabled {
        build_read_set_registry(module, cfg.ipa_iter_limit)
    } else {
        HashMap::new()
    };

    for item in &mut module.items {
        if let Item::Fn(f) = item {
            if cfg.licm_enabled {
                licm_fn_with_ipa(f, &registry, cfg, &write_sets, &read_sets);
            }
            if cfg.chain_enabled {
                chain_cache_fn_with_ipa(f, &registry, cfg, &write_sets, &read_sets);
            }
            if cfg.pure_enabled {
                pure_cache_fn_with_ipa(f, &registry, &pure_methods, cfg, &write_sets, &read_sets);
            }
            cache_fn_ipa(f, &registry, &write_sets, &read_sets, cfg);
        }
    }
    for pf in &mut module.peer_files {
        for item in &mut pf.items_here {
            if let Item::Fn(f) = item {
                if cfg.licm_enabled {
                    licm_fn_with_ipa(f, &registry, cfg, &write_sets, &read_sets);
                }
                if cfg.chain_enabled {
                    chain_cache_fn_with_ipa(f, &registry, cfg, &write_sets, &read_sets);
                }
                if cfg.pure_enabled {
                    pure_cache_fn_with_ipa(f, &registry, &pure_methods, cfg, &write_sets, &read_sets);
                }
                cache_fn_ipa(f, &registry, &write_sets, &read_sets, cfg);
            }
        }
    }
}

/// Plan 123.7 (D223): public API for IPA write-set inference.
/// Returns per-method field-write-set map.
///
/// Each key `(type_name, method_name)` maps to the set of field
/// names that the method's body writes via `@F = ...` (top-level
/// Assign with target = `Member{SelfAccess, F}`). Transitively
/// includes fields written by methods called from the body
/// (computed via iterative closure ≤ 10 iterations).
///
/// Used by:
/// - V7 IPA refinement (Plan 123.7) — refines mut field cache
///   barrier check.
/// - `nova check --explain-cache` extended in V5.1 to show callee
///   write-sets.
pub fn module_write_sets(module: &Module) -> HashMap<(String, String), HashSet<String>> {
    let cfg = FieldCacheConfig::default();
    build_write_set_registry(module, cfg.ipa_iter_limit)
}

/// Plan 123.7 (D223): build per-method field-write-set.
/// Walks each FnDecl with receiver; collects fields that body assigns
/// to via `@F = ...` (top-level Assign with Member{SelfAccess, F}
/// target). Recursive transitive closure через method calls — V7
/// conservative: if body contains a Call to another method,
/// we union with the target's write set (single-pass approximation —
/// for fully precise SCC closure see V7.3 followup).
///
/// Plan 123.6.3 (V6.3, 2026-06-02): `iter_limit` now configurable
/// (default 10) — caps the fixed-point iterations. For most modules
/// the closure converges in ≤3 iterations; larger limits are
/// forensic-only.
fn build_write_set_registry(module: &Module, iter_limit: usize) -> HashMap<(String, String), HashSet<String>> {
    let mut direct: HashMap<(String, String), HashSet<String>> = HashMap::new();
    // Track callees per method for second pass.
    let mut callees: HashMap<(String, String), HashSet<(String, String)>> = HashMap::new();

    collect_direct_writes(&module.items, &mut direct, &mut callees);
    for pf in &module.peer_files {
        collect_direct_writes(&pf.items_here, &mut direct, &mut callees);
    }

    // Iterative closure (≤ iter_limit iterations bound; converges for
    // typical call graphs in ≤3).
    for _ in 0..iter_limit {
        let mut changed = false;
        for (key, callees_set) in &callees {
            for callee in callees_set {
                if let Some(callee_writes) = direct.get(callee).cloned() {
                    let entry = direct.entry(key.clone()).or_default();
                    for f in callee_writes {
                        if entry.insert(f) {
                            changed = true;
                        }
                    }
                }
            }
        }
        if !changed { break; }
    }
    direct
}

fn collect_direct_writes(
    items: &[Item],
    direct: &mut HashMap<(String, String), HashSet<String>>,
    callees: &mut HashMap<(String, String), HashSet<(String, String)>>,
) {
    for item in items {
        if let Item::Fn(f) = item {
            if let Some(recv) = &f.receiver {
                if recv.kind != ReceiverKind::Instance {
                    continue;
                }
                let key = (recv.type_name.clone(), f.name.clone());
                let mut writes = HashSet::new();
                let mut method_callees = HashSet::new();
                match &f.body {
                    FnBody::Block(b) => collect_writes_block(b, &recv.type_name, &mut writes, &mut method_callees),
                    FnBody::Expr(e) => collect_writes_expr(e, &recv.type_name, &mut writes, &mut method_callees),
                    FnBody::External => {}
                }
                direct.insert(key.clone(), writes);
                callees.insert(key, method_callees);
            }
        }
    }
}

fn collect_writes_block(
    b: &Block,
    recv_type: &str,
    writes: &mut HashSet<String>,
    callees: &mut HashSet<(String, String)>,
) {
    for s in &b.stmts {
        collect_writes_stmt(s, recv_type, writes, callees);
    }
    if let Some(t) = &b.trailing {
        collect_writes_expr(t, recv_type, writes, callees);
    }
}

fn collect_writes_stmt(
    s: &Stmt,
    recv_type: &str,
    writes: &mut HashSet<String>,
    callees: &mut HashSet<(String, String)>,
) {
    match s {
        Stmt::Assign { target, value, .. } => {
            if let Some(fname) = match_self_field(target) {
                writes.insert(fname.to_string());
            } else {
                collect_writes_expr(target, recv_type, writes, callees);
            }
            collect_writes_expr(value, recv_type, writes, callees);
        }
        Stmt::Let(d) => collect_writes_expr(&d.value, recv_type, writes, callees),
        Stmt::Const(d) => collect_writes_expr(&d.value, recv_type, writes, callees),
        Stmt::Expr(e) => collect_writes_expr(e, recv_type, writes, callees),
        Stmt::Return { value, .. } => {
            if let Some(v) = value { collect_writes_expr(v, recv_type, writes, callees); }
        }
        Stmt::Throw { value, .. } => collect_writes_expr(value, recv_type, writes, callees),
        Stmt::Defer { body, .. } | Stmt::ErrDefer { body, .. }
        | Stmt::OkDefer { body, .. } | Stmt::DeferWithResult { body, .. } => {
            collect_writes_expr(body, recv_type, writes, callees);
        }
        Stmt::ConsumeScope { init, body, .. } => {
            collect_writes_expr(init, recv_type, writes, callees);
            collect_writes_block(body, recv_type, writes, callees);
        }
        Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => {
            collect_writes_expr(expr, recv_type, writes, callees);
        }
        Stmt::Break(_) | Stmt::Continue(_)
        | Stmt::Apply { .. } | Stmt::Calc { .. } | Stmt::Reveal { .. } => {}
    }
}

fn collect_writes_expr(
    e: &Expr,
    recv_type: &str,
    writes: &mut HashSet<String>,
    callees: &mut HashSet<(String, String)>,
) {
    // Detect self-method calls — `@<method>(args)` Call where func is
    // Member{SelfAccess, name}. Add (recv_type, method) к callees.
    if let ExprKind::Call { func, args, trailing } = &e.kind {
        if let ExprKind::Member { obj, name } = &func.kind {
            if matches!(obj.kind, ExprKind::SelfAccess) {
                callees.insert((recv_type.to_string(), name.clone()));
            }
        }
        // Continue recurse.
        if let ExprKind::Member { obj, .. } = &func.kind {
            collect_writes_expr(obj, recv_type, writes, callees);
        } else {
            collect_writes_expr(func, recv_type, writes, callees);
        }
        for a in args {
            collect_writes_expr(a.expr(), recv_type, writes, callees);
        }
        if let Some(t) = trailing {
            match t {
                Trailing::Block(b) => collect_writes_block(b, recv_type, writes, callees),
                _ => {}
            }
        }
        return;
    }
    match &e.kind {
        ExprKind::Block(b) => collect_writes_block(b, recv_type, writes, callees),
        ExprKind::If { cond, then, else_ } => {
            collect_writes_expr(cond, recv_type, writes, callees);
            collect_writes_block(then, recv_type, writes, callees);
            if let Some(eb) = else_ {
                match eb {
                    ElseBranch::Block(b) => collect_writes_block(b, recv_type, writes, callees),
                    ElseBranch::If(e) => collect_writes_expr(e, recv_type, writes, callees),
                }
            }
        }
        ExprKind::IfLet { scrutinee, then, else_, .. } => {
            collect_writes_expr(scrutinee, recv_type, writes, callees);
            collect_writes_block(then, recv_type, writes, callees);
            if let Some(eb) = else_ {
                match eb {
                    ElseBranch::Block(b) => collect_writes_block(b, recv_type, writes, callees),
                    ElseBranch::If(e) => collect_writes_expr(e, recv_type, writes, callees),
                }
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            collect_writes_expr(scrutinee, recv_type, writes, callees);
            for arm in arms {
                if let Some(g) = &arm.guard { collect_writes_expr(g, recv_type, writes, callees); }
                match &arm.body {
                    MatchArmBody::Expr(e) => collect_writes_expr(e, recv_type, writes, callees),
                    MatchArmBody::Block(b) => collect_writes_block(b, recv_type, writes, callees),
                }
            }
        }
        ExprKind::For { iter, body, .. } | ExprKind::ParallelFor { iter, body, .. } => {
            collect_writes_expr(iter, recv_type, writes, callees);
            collect_writes_block(body, recv_type, writes, callees);
        }
        ExprKind::While { cond, body, .. } => {
            collect_writes_expr(cond, recv_type, writes, callees);
            collect_writes_block(body, recv_type, writes, callees);
        }
        ExprKind::WhileLet { scrutinee, body, .. } => {
            collect_writes_expr(scrutinee, recv_type, writes, callees);
            collect_writes_block(body, recv_type, writes, callees);
        }
        ExprKind::Loop { body, .. } => collect_writes_block(body, recv_type, writes, callees),
        ExprKind::With { bindings, body } => {
            for wb in bindings { collect_writes_expr(&wb.handler, recv_type, writes, callees); }
            collect_writes_block(body, recv_type, writes, callees);
        }
        ExprKind::Forbid { body, .. } | ExprKind::Realtime { body, .. }
        | ExprKind::Detach(body) | ExprKind::Blocking(body) => {
            collect_writes_block(body, recv_type, writes, callees);
        }
        ExprKind::Supervised { body, cancel } => {
            collect_writes_block(body, recv_type, writes, callees);
            if let Some(c) = cancel { collect_writes_expr(c, recv_type, writes, callees); }
        }
        ExprKind::Spawn(e) | ExprKind::Throw(e) | ExprKind::Try(e) | ExprKind::Bang(e)
        | ExprKind::Member { obj: e, .. } | ExprKind::TurboFish { base: e, .. }
        | ExprKind::As(e, _) | ExprKind::Is(e, _) | ExprKind::Unary { operand: e, .. } => {
            collect_writes_expr(e, recv_type, writes, callees);
        }
        ExprKind::Coalesce(a, b) | ExprKind::Binary { left: a, right: b, .. } => {
            collect_writes_expr(a, recv_type, writes, callees);
            collect_writes_expr(b, recv_type, writes, callees);
        }
        ExprKind::Index { obj, index } => {
            collect_writes_expr(obj, recv_type, writes, callees);
            collect_writes_expr(index, recv_type, writes, callees);
        }
        _ => {} // Other expression types — closures, literals etc. — skip.
    }
}

/// Plan 123.7.1: build read-set registry — fields each method reads.
/// Parallel infrastructure к write_sets для V3.1 frame-based
/// pure-cache invalidation.
fn build_read_set_registry(module: &Module, iter_limit: usize) -> HashMap<(String, String), HashSet<String>> {
    let mut direct: HashMap<(String, String), HashSet<String>> = HashMap::new();
    let mut callees: HashMap<(String, String), HashSet<(String, String)>> = HashMap::new();
    collect_direct_reads(&module.items, &mut direct, &mut callees);
    for pf in &module.peer_files {
        collect_direct_reads(&pf.items_here, &mut direct, &mut callees);
    }
    // Plan 123.6.3 (V6.3): iter_limit configurable (default 10).
    // Same pattern as write_sets.
    for _ in 0..iter_limit {
        let mut changed = false;
        for (key, callees_set) in &callees {
            for callee in callees_set {
                if let Some(callee_reads) = direct.get(callee).cloned() {
                    let entry = direct.entry(key.clone()).or_default();
                    for f in callee_reads {
                        if entry.insert(f) { changed = true; }
                    }
                }
            }
        }
        if !changed { break; }
    }
    direct
}

fn collect_direct_reads(
    items: &[Item],
    direct: &mut HashMap<(String, String), HashSet<String>>,
    callees: &mut HashMap<(String, String), HashSet<(String, String)>>,
) {
    for item in items {
        if let Item::Fn(f) = item {
            if let Some(recv) = &f.receiver {
                if recv.kind != ReceiverKind::Instance { continue; }
                let key = (recv.type_name.clone(), f.name.clone());
                let mut reads = HashSet::new();
                let mut method_callees = HashSet::new();
                match &f.body {
                    FnBody::Block(b) => collect_reads_block(b, &recv.type_name, &mut reads, &mut method_callees),
                    FnBody::Expr(e) => collect_reads_expr(e, &recv.type_name, &mut reads, &mut method_callees),
                    FnBody::External => {}
                }
                direct.insert(key.clone(), reads);
                callees.insert(key, method_callees);
            }
        }
    }
}

fn collect_reads_block(b: &Block, recv_type: &str, reads: &mut HashSet<String>, callees: &mut HashSet<(String, String)>) {
    for s in &b.stmts { collect_reads_stmt(s, recv_type, reads, callees); }
    if let Some(t) = &b.trailing { collect_reads_expr(t, recv_type, reads, callees); }
}

fn collect_reads_stmt(s: &Stmt, recv_type: &str, reads: &mut HashSet<String>, callees: &mut HashSet<(String, String)>) {
    match s {
        Stmt::Assign { target, value, .. } => {
            // The target's @F = ... is a WRITE, not a read; skip target.
            // But target could be e.g. `@a[i] = ...` — `@a` and `i` ARE reads.
            // For V3.1 simplicity: skip plain `@F = ...` targets, recurse otherwise.
            if match_self_field(target).is_none() {
                collect_reads_expr(target, recv_type, reads, callees);
            }
            collect_reads_expr(value, recv_type, reads, callees);
        }
        Stmt::Let(d) => collect_reads_expr(&d.value, recv_type, reads, callees),
        Stmt::Const(d) => collect_reads_expr(&d.value, recv_type, reads, callees),
        Stmt::Expr(e) => collect_reads_expr(e, recv_type, reads, callees),
        Stmt::Return { value, .. } => {
            if let Some(v) = value { collect_reads_expr(v, recv_type, reads, callees); }
        }
        Stmt::Throw { value, .. } => collect_reads_expr(value, recv_type, reads, callees),
        Stmt::Defer { body, .. } | Stmt::ErrDefer { body, .. }
        | Stmt::OkDefer { body, .. } | Stmt::DeferWithResult { body, .. } => {
            collect_reads_expr(body, recv_type, reads, callees);
        }
        Stmt::ConsumeScope { init, body, .. } => {
            collect_reads_expr(init, recv_type, reads, callees);
            collect_reads_block(body, recv_type, reads, callees);
        }
        Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => {
            collect_reads_expr(expr, recv_type, reads, callees);
        }
        Stmt::Break(_) | Stmt::Continue(_)
        | Stmt::Apply { .. } | Stmt::Calc { .. } | Stmt::Reveal { .. } => {}
    }
}

fn collect_reads_expr(e: &Expr, recv_type: &str, reads: &mut HashSet<String>, callees: &mut HashSet<(String, String)>) {
    // Detect `@F` read.
    if let Some(fname) = match_self_field(e) {
        reads.insert(fname.to_string());
        return;
    }
    // Detect `@<method>(args)` call → callee tracking.
    if let ExprKind::Call { func, args, trailing } = &e.kind {
        if let ExprKind::Member { obj, name } = &func.kind {
            if matches!(obj.kind, ExprKind::SelfAccess) {
                callees.insert((recv_type.to_string(), name.clone()));
            }
            collect_reads_expr(obj, recv_type, reads, callees);
        } else {
            collect_reads_expr(func, recv_type, reads, callees);
        }
        for a in args { collect_reads_expr(a.expr(), recv_type, reads, callees); }
        if let Some(t) = trailing {
            if let Trailing::Block(b) = t { collect_reads_block(b, recv_type, reads, callees); }
        }
        return;
    }
    match &e.kind {
        ExprKind::Block(b) => collect_reads_block(b, recv_type, reads, callees),
        ExprKind::If { cond, then, else_ } => {
            collect_reads_expr(cond, recv_type, reads, callees);
            collect_reads_block(then, recv_type, reads, callees);
            if let Some(eb) = else_ {
                match eb {
                    ElseBranch::Block(b) => collect_reads_block(b, recv_type, reads, callees),
                    ElseBranch::If(e) => collect_reads_expr(e, recv_type, reads, callees),
                }
            }
        }
        ExprKind::IfLet { scrutinee, then, else_, .. } => {
            collect_reads_expr(scrutinee, recv_type, reads, callees);
            collect_reads_block(then, recv_type, reads, callees);
            if let Some(eb) = else_ {
                match eb {
                    ElseBranch::Block(b) => collect_reads_block(b, recv_type, reads, callees),
                    ElseBranch::If(e) => collect_reads_expr(e, recv_type, reads, callees),
                }
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            collect_reads_expr(scrutinee, recv_type, reads, callees);
            for arm in arms {
                if let Some(g) = &arm.guard { collect_reads_expr(g, recv_type, reads, callees); }
                match &arm.body {
                    MatchArmBody::Expr(e) => collect_reads_expr(e, recv_type, reads, callees),
                    MatchArmBody::Block(b) => collect_reads_block(b, recv_type, reads, callees),
                }
            }
        }
        ExprKind::For { iter, body, .. } | ExprKind::ParallelFor { iter, body, .. } => {
            collect_reads_expr(iter, recv_type, reads, callees);
            collect_reads_block(body, recv_type, reads, callees);
        }
        ExprKind::While { cond, body, .. } => {
            collect_reads_expr(cond, recv_type, reads, callees);
            collect_reads_block(body, recv_type, reads, callees);
        }
        ExprKind::WhileLet { scrutinee, body, .. } => {
            collect_reads_expr(scrutinee, recv_type, reads, callees);
            collect_reads_block(body, recv_type, reads, callees);
        }
        ExprKind::Loop { body, .. } => collect_reads_block(body, recv_type, reads, callees),
        ExprKind::With { bindings, body } => {
            for wb in bindings { collect_reads_expr(&wb.handler, recv_type, reads, callees); }
            collect_reads_block(body, recv_type, reads, callees);
        }
        ExprKind::Forbid { body, .. } | ExprKind::Realtime { body, .. }
        | ExprKind::Detach(body) | ExprKind::Blocking(body) => {
            collect_reads_block(body, recv_type, reads, callees);
        }
        ExprKind::Supervised { body, cancel } => {
            collect_reads_block(body, recv_type, reads, callees);
            if let Some(c) = cancel { collect_reads_expr(c, recv_type, reads, callees); }
        }
        ExprKind::Spawn(e) | ExprKind::Throw(e) | ExprKind::Try(e) | ExprKind::Bang(e)
        | ExprKind::Member { obj: e, .. } | ExprKind::TurboFish { base: e, .. }
        | ExprKind::As(e, _) | ExprKind::Is(e, _) | ExprKind::Unary { operand: e, .. } => {
            collect_reads_expr(e, recv_type, reads, callees);
        }
        ExprKind::Coalesce(a, b) | ExprKind::Binary { left: a, right: b, .. } => {
            collect_reads_expr(a, recv_type, reads, callees);
            collect_reads_expr(b, recv_type, reads, callees);
        }
        ExprKind::Index { obj, index } => {
            collect_reads_expr(obj, recv_type, reads, callees);
            collect_reads_expr(index, recv_type, reads, callees);
        }
        ExprKind::Range { start, end, .. } => {
            if let Some(s) = start { collect_reads_expr(s, recv_type, reads, callees); }
            if let Some(e) = end { collect_reads_expr(e, recv_type, reads, callees); }
        }
        ExprKind::Forall { range, body, .. } | ExprKind::Exists { range, body, .. } => {
            collect_reads_expr(range, recv_type, reads, callees);
            collect_reads_expr(body, recv_type, reads, callees);
        }
        ExprKind::ArrayLit(elems) => {
            for el in elems {
                match el {
                    ArrayElem::Item(e) | ArrayElem::Spread(e) => collect_reads_expr(e, recv_type, reads, callees),
                }
            }
        }
        ExprKind::TupleLit(elems) => {
            for el in elems { collect_reads_expr(el, recv_type, reads, callees); }
        }
        ExprKind::RecordLit { fields: rfields, .. } => {
            for rf in rfields {
                if let Some(v) = &rf.value { collect_reads_expr(v, recv_type, reads, callees); }
            }
        }
        _ => {} // Closures + literals.
    }
}

/// Plan 123 V7.2 (2026-06-02): explicit IPA wrappers — pre-build the
/// `IpaCtx<'_>` borrow and pass it as parameter to the impl. Replaces
/// V7.1 thread-local plumbing (LICM_WRITE_SETS / PURE_IPA_CTX /
/// CHAIN_IPA_CTX removed). `recv_type` is cloned into a local String
/// up-front to avoid aliasing `&f` borrow with the `&mut f` passed
/// downward to `*_impl`.
fn licm_fn_with_ipa(
    f: &mut FnDecl,
    reg: &FieldRegistry,
    cfg: &FieldCacheConfig,
    write_sets: &HashMap<(String, String), HashSet<String>>,
    read_sets: &HashMap<(String, String), HashSet<String>>,
) {
    let recv_type = match recv_type_for_ipa(f, cfg, write_sets) {
        Some(rt) => rt,
        None => { licm_fn_impl(f, reg, cfg, None); return; }
    };
    let ipa = IpaCtx { write_sets, recv_type: recv_type.as_str(), read_sets };
    licm_fn_impl(f, reg, cfg, Some(ipa))
}

fn pure_cache_fn_with_ipa(
    f: &mut FnDecl,
    reg: &FieldRegistry,
    pure_methods: &HashSet<(String, String)>,
    cfg: &FieldCacheConfig,
    write_sets: &HashMap<(String, String), HashSet<String>>,
    read_sets: &HashMap<(String, String), HashSet<String>>,
) {
    let recv_type = match recv_type_for_ipa(f, cfg, write_sets) {
        Some(rt) => rt,
        None => { pure_cache_fn_impl(f, reg, pure_methods, cfg, None); return; }
    };
    let ipa = IpaCtx { write_sets, recv_type: recv_type.as_str(), read_sets };
    pure_cache_fn_impl(f, reg, pure_methods, cfg, Some(ipa))
}

fn chain_cache_fn_with_ipa(
    f: &mut FnDecl,
    reg: &FieldRegistry,
    cfg: &FieldCacheConfig,
    write_sets: &HashMap<(String, String), HashSet<String>>,
    read_sets: &HashMap<(String, String), HashSet<String>>,
) {
    let recv_type = match recv_type_for_ipa(f, cfg, write_sets) {
        Some(rt) => rt,
        None => { chain_cache_fn_impl(f, reg, cfg, None); return; }
    };
    let ipa = IpaCtx { write_sets, recv_type: recv_type.as_str(), read_sets };
    chain_cache_fn_impl(f, reg, cfg, Some(ipa))
}

/// Plan 123 V7.2 (2026-06-02): clone recv_type into a local String when
/// IPA is enabled and applicable. Returning `Option<String>` (vs &str)
/// detaches the borrow from `f`, so the caller can later pass `&mut f`
/// to the impl without aliasing conflicts.
fn recv_type_for_ipa(
    f: &FnDecl,
    cfg: &FieldCacheConfig,
    write_sets: &HashMap<(String, String), HashSet<String>>,
) -> Option<String> {
    if !cfg.ipa_enabled || write_sets.is_empty() {
        return None;
    }
    f.receiver.as_ref().map(|r| r.type_name.clone())
}

/// Plan 123.7.1 (V7.1): IPA context — threading write_sets +
/// receiver type through barrier-checking helpers. Optional via
/// `Option<IpaCtx<'a>>` parameter — `None` = legacy V1-V6
/// conservative "any call = barrier" behavior.
#[derive(Clone, Copy)]
pub(crate) struct IpaCtx<'a> {
    pub write_sets: &'a HashMap<(String, String), HashSet<String>>,
    pub recv_type: &'a str,
    /// V7.1 Ф.3: pure-method field-read-set lookup.
    /// (recv_type, method_name) → set of fields method reads.
    pub read_sets: &'a HashMap<(String, String), HashSet<String>>,
}

impl<'a> IpaCtx<'a> {
    /// True if calling `(recv_type, method_name)` invalidates cache
    /// для field `fname` per IPA write_set lookup.
    /// Returns true if method unknown (conservative).
    pub(crate) fn call_invalidates_field(&self, method_name: &str, fname: &str) -> bool {
        match self.write_sets.get(&(self.recv_type.to_string(), method_name.to_string())) {
            Some(ws) => ws.contains(fname),
            None => true, // unknown callee — conservative.
        }
    }
}

/// Plan 123.7: cache_fn extended с IPA-aware mut barrier.
/// V7.1: full integration — passes IpaCtx through to barrier helpers.
fn cache_fn_ipa(
    f: &mut FnDecl,
    reg: &FieldRegistry,
    write_sets: &HashMap<(String, String), HashSet<String>>,
    read_sets: &HashMap<(String, String), HashSet<String>>,
    cfg: &FieldCacheConfig,
) {
    if !cfg.ipa_enabled || write_sets.is_empty() {
        cache_fn(f, reg, cfg);
        return;
    }
    let Some(recv) = &f.receiver else {
        cache_fn(f, reg, cfg);
        return;
    };
    let recv_type = recv.type_name.clone();
    let ipa = IpaCtx { write_sets, recv_type: &recv_type, read_sets };
    cache_fn_with_ipa(f, reg, cfg, Some(ipa));
}

/// Plan 123.3: build pure-method registry. Includes only methods с
/// `purity == Purity::Pure` AND args-less (V3 scope).
fn build_pure_methods_registry(module: &Module) -> HashSet<(String, String)> {
    let mut out: HashSet<(String, String)> = HashSet::new();
    register_pure_items(&module.items, &mut out);
    for pf in &module.peer_files {
        register_pure_items(&pf.items_here, &mut out);
    }
    out
}

fn register_pure_items(items: &[Item], out: &mut HashSet<(String, String)>) {
    for item in items {
        if let Item::Fn(f) = item {
            if let Some(recv) = &f.receiver {
                // V3 + V3.1: pure + instance method. Args-less (V3) and
                // any-args (V3.1 — args validation at call site через
                // literal check).
                if f.purity == Purity::Pure
                    && recv.kind == ReceiverKind::Instance
                {
                    out.insert((recv.type_name.clone(), f.name.clone()));
                }
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
    cache_fn_with_ipa(f, reg, cfg, None);
}

/// Plan 123.7.1: cache_fn variant that accepts optional IpaCtx.
/// When `ipa = Some(ctx)`, mut field prefix region is computed с
/// IPA-aware barrier check — self-method calls don't count as
/// barriers if callee doesn't write the field.
fn cache_fn_with_ipa(
    f: &mut FnDecl,
    reg: &FieldRegistry,
    cfg: &FieldCacheConfig,
    ipa: Option<IpaCtx<'_>>,
) {
    let Some(recv) = &f.receiver else { return };
    if recv.kind == ReceiverKind::Static {
        return;
    }
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
    if f.is_external {
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

    let mut local_names: HashSet<String> = HashSet::new();
    for p in &f.params {
        local_names.insert(p.name.clone());
    }
    collect_local_names_fn(f, &mut local_names);

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
                // Plan 123.7.1: pass ipa context для refined barrier check.
                if let Some(prefix_count) = count_mut_prefix_reads_with_ipa(&f.body, fname, ipa) {
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

    rewrite_fn_body_split_with_ipa(f, &ro_candidates, &mut_candidates, &name_map, ipa);
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
#[allow(dead_code)]
fn count_mut_prefix_reads(body: &FnBody, fname: &str) -> Option<usize> {
    count_mut_prefix_reads_with_ipa(body, fname, None)
}

/// Plan 123.7.1: IPA-aware mut prefix region scanner. When `ipa =
/// Some(ctx)`, self-method calls don't count as barrier if callee's
/// write_set excludes `fname`.
fn count_mut_prefix_reads_with_ipa(
    body: &FnBody,
    fname: &str,
    ipa: Option<IpaCtx<'_>>,
) -> Option<usize> {
    match body {
        FnBody::Block(b) => {
            let mut count = 0usize;
            for s in &b.stmts {
                if stmt_is_barrier_for_with_ipa(s, fname, ipa) {
                    return Some(count);
                }
                count += count_field_reads_in_stmt(s, fname);
            }
            if let Some(t) = &b.trailing {
                if expr_is_barrier_for_with_ipa(t, fname, ipa) {
                    return Some(count);
                }
                count += count_field_reads_in_expr(t, fname);
            }
            Some(count)
        }
        FnBody::Expr(e) => {
            if expr_is_barrier_for_with_ipa(e, fname, ipa) {
                Some(0)
            } else {
                Some(count_field_reads_in_expr(e, fname))
            }
        }
        FnBody::External => None,
    }
}

#[allow(dead_code)]
fn stmt_is_barrier_for(s: &Stmt, fname: &str) -> bool {
    stmt_is_barrier_for_with_ipa(s, fname, None)
}

#[allow(dead_code)]
fn expr_is_barrier_for(e: &Expr, fname: &str) -> bool {
    expr_is_barrier_for_with_ipa(e, fname, None)
}

/// Plan 123.7.1: IPA-aware barrier check для Stmt.
fn stmt_is_barrier_for_with_ipa(s: &Stmt, fname: &str, ipa: Option<IpaCtx<'_>>) -> bool {
    if stmt_has_write_to(s, fname) {
        return true;
    }
    stmt_contains_invalidating_call_for(s, fname, ipa)
}

fn expr_is_barrier_for_with_ipa(e: &Expr, fname: &str, ipa: Option<IpaCtx<'_>>) -> bool {
    expr_contains_write_to(e, fname) || expr_contains_invalidating_call_for(e, fname, ipa)
}

/// Plan 123.7.1: returns true if stmt contains a Call that
/// invalidates cache for field `fname`. Without IPA: any Call
/// invalidates. With IPA: self-method calls check write_set.
fn stmt_contains_invalidating_call_for(
    s: &Stmt,
    fname: &str,
    ipa: Option<IpaCtx<'_>>,
) -> bool {
    match s {
        Stmt::Let(d) => expr_contains_invalidating_call_for(&d.value, fname, ipa),
        Stmt::Const(d) => expr_contains_invalidating_call_for(&d.value, fname, ipa),
        Stmt::Expr(e) => expr_contains_invalidating_call_for(e, fname, ipa),
        Stmt::Assign { target, value, .. } => {
            expr_contains_invalidating_call_for(target, fname, ipa)
                || expr_contains_invalidating_call_for(value, fname, ipa)
        }
        Stmt::Return { value, .. } => {
            value.as_ref().map_or(false, |v| expr_contains_invalidating_call_for(v, fname, ipa))
        }
        Stmt::Throw { value, .. } => expr_contains_invalidating_call_for(value, fname, ipa),
        Stmt::Defer { body, .. } | Stmt::ErrDefer { body, .. }
        | Stmt::OkDefer { body, .. } | Stmt::DeferWithResult { body, .. } => {
            expr_contains_invalidating_call_for(body, fname, ipa)
        }
        Stmt::ConsumeScope { init, body, .. } => {
            expr_contains_invalidating_call_for(init, fname, ipa)
                || block_contains_invalidating_call_for(body, fname, ipa)
        }
        Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => {
            expr_contains_invalidating_call_for(expr, fname, ipa)
        }
        Stmt::Break(_) | Stmt::Continue(_)
        | Stmt::Apply { .. } | Stmt::Calc { .. } | Stmt::Reveal { .. } => false,
    }
}

fn block_contains_invalidating_call_for(b: &Block, fname: &str, ipa: Option<IpaCtx<'_>>) -> bool {
    b.stmts.iter().any(|s| stmt_contains_invalidating_call_for(s, fname, ipa))
        || b.trailing.as_ref().map_or(false, |t| expr_contains_invalidating_call_for(t, fname, ipa))
}

/// Plan 123.7.1: IPA-aware call detection. For Call expressions:
/// - Self-method call `@<M>(args)` where (recv_type, M) ∈ write_sets:
///   invalidates only if fname ∈ write_set.
/// - Unknown self-methods → conservative invalidate.
/// - Non-self calls → invalidate (caller doesn't know callee).
/// - Spawn/Supervised/etc → invalidate.
fn expr_contains_invalidating_call_for(
    e: &Expr,
    fname: &str,
    ipa: Option<IpaCtx<'_>>,
) -> bool {
    match &e.kind {
        ExprKind::Call { func, args, trailing } => {
            // Check if THIS call invalidates.
            let this_call_invalidates = if let Some(ctx) = ipa {
                if let ExprKind::Member { obj, name: m } = &func.kind {
                    if matches!(obj.kind, ExprKind::SelfAccess) {
                        ctx.call_invalidates_field(m, fname)
                    } else {
                        // Non-self method dispatch (e.g. var.method())
                        // — conservative invalidate.
                        true
                    }
                } else {
                    // Free fn / static / etc — conservative.
                    true
                }
            } else {
                // No IPA context — conservative (V1 behavior).
                true
            };
            if this_call_invalidates {
                return true;
            }
            // Even if this call doesn't invalidate, nested calls in
            // args might. Recurse.
            for a in args {
                if expr_contains_invalidating_call_for(a.expr(), fname, ipa) {
                    return true;
                }
            }
            if let Some(t) = trailing {
                match t {
                    Trailing::Block(b) => {
                        if block_contains_invalidating_call_for(b, fname, ipa) { return true; }
                    }
                    Trailing::Fn(sb) => {
                        // Trailing closure — bodies are values, evaluation
                        // by callee. Conservative — but не more invalidating
                        // than the Call itself already evaluated.
                        let _ = sb;
                    }
                    Trailing::LegacyBlockWithParams(_) => {}
                }
            }
            // Also obj of Member func may contain calls (e.g.
            // `expr.method()` where expr is itself a Call).
            if let ExprKind::Member { obj, .. } = &func.kind {
                if expr_contains_invalidating_call_for(obj, fname, ipa) {
                    return true;
                }
            } else {
                if expr_contains_invalidating_call_for(func, fname, ipa) {
                    return true;
                }
            }
            false
        }
        // Concurrency/effects always invalidate.
        ExprKind::Spawn(_) | ExprKind::Supervised { .. } | ExprKind::Detach(_)
        | ExprKind::Blocking(_) | ExprKind::With { .. } => true,
        // Compound walks below.
        ExprKind::Block(b) => block_contains_invalidating_call_for(b, fname, ipa),
        ExprKind::If { cond, then, else_ } => {
            expr_contains_invalidating_call_for(cond, fname, ipa)
                || block_contains_invalidating_call_for(then, fname, ipa)
                || else_.as_ref().map_or(false, |eb| match eb {
                    ElseBranch::Block(b) => block_contains_invalidating_call_for(b, fname, ipa),
                    ElseBranch::If(e) => expr_contains_invalidating_call_for(e, fname, ipa),
                })
        }
        ExprKind::IfLet { scrutinee, then, else_, .. } => {
            expr_contains_invalidating_call_for(scrutinee, fname, ipa)
                || block_contains_invalidating_call_for(then, fname, ipa)
                || else_.as_ref().map_or(false, |eb| match eb {
                    ElseBranch::Block(b) => block_contains_invalidating_call_for(b, fname, ipa),
                    ElseBranch::If(e) => expr_contains_invalidating_call_for(e, fname, ipa),
                })
        }
        ExprKind::Match { scrutinee, arms } => {
            expr_contains_invalidating_call_for(scrutinee, fname, ipa)
                || arms.iter().any(|arm| {
                    arm.guard.as_ref().map_or(false, |g| expr_contains_invalidating_call_for(g, fname, ipa))
                        || match &arm.body {
                            MatchArmBody::Expr(e) => expr_contains_invalidating_call_for(e, fname, ipa),
                            MatchArmBody::Block(b) => block_contains_invalidating_call_for(b, fname, ipa),
                        }
                })
        }
        ExprKind::For { iter, body, .. } | ExprKind::ParallelFor { iter, body, .. } => {
            expr_contains_invalidating_call_for(iter, fname, ipa)
                || block_contains_invalidating_call_for(body, fname, ipa)
        }
        ExprKind::While { cond, body, .. } => {
            expr_contains_invalidating_call_for(cond, fname, ipa)
                || block_contains_invalidating_call_for(body, fname, ipa)
        }
        ExprKind::WhileLet { scrutinee, body, .. } => {
            expr_contains_invalidating_call_for(scrutinee, fname, ipa)
                || block_contains_invalidating_call_for(body, fname, ipa)
        }
        ExprKind::Loop { body, .. } => block_contains_invalidating_call_for(body, fname, ipa),
        ExprKind::Forbid { body, .. } | ExprKind::Realtime { body, .. } => {
            block_contains_invalidating_call_for(body, fname, ipa)
        }
        ExprKind::Throw(e) | ExprKind::Try(e) | ExprKind::Bang(e)
        | ExprKind::Member { obj: e, .. } | ExprKind::TurboFish { base: e, .. }
        | ExprKind::As(e, _) | ExprKind::Is(e, _) | ExprKind::Unary { operand: e, .. } => {
            expr_contains_invalidating_call_for(e, fname, ipa)
        }
        ExprKind::Coalesce(a, b) | ExprKind::Binary { left: a, right: b, .. } => {
            expr_contains_invalidating_call_for(a, fname, ipa)
                || expr_contains_invalidating_call_for(b, fname, ipa)
        }
        ExprKind::Index { obj, index } => {
            expr_contains_invalidating_call_for(obj, fname, ipa)
                || expr_contains_invalidating_call_for(index, fname, ipa)
        }
        ExprKind::ArrayLit(elems) => elems.iter().any(|el| match el {
            ArrayElem::Item(e) | ArrayElem::Spread(e) => expr_contains_invalidating_call_for(e, fname, ipa),
        }),
        ExprKind::MapLit { elems, .. } => elems.iter().any(|el| match el {
            MapElem::Pair(k, v) => expr_contains_invalidating_call_for(k, fname, ipa) || expr_contains_invalidating_call_for(v, fname, ipa),
            MapElem::Spread(e) => expr_contains_invalidating_call_for(e, fname, ipa),
        }),
        ExprKind::RecordLit { fields: rfields, .. } => rfields.iter().any(|rf| {
            rf.value.as_ref().map_or(false, |v| expr_contains_invalidating_call_for(v, fname, ipa))
        }),
        ExprKind::TupleLit(elems) => elems.iter().any(|el| expr_contains_invalidating_call_for(el, fname, ipa)),
        ExprKind::InterpolatedStr { parts } => parts.iter().any(|p| {
            if let InterpStrPart::Expr(e) = p { expr_contains_invalidating_call_for(e, fname, ipa) } else { false }
        }),
        ExprKind::Select { .. } => true,
        ExprKind::Range { start, end, .. } => {
            start.as_ref().map_or(false, |s| expr_contains_invalidating_call_for(s, fname, ipa))
                || end.as_ref().map_or(false, |e| expr_contains_invalidating_call_for(e, fname, ipa))
        }
        ExprKind::Forall { range, body, .. } | ExprKind::Exists { range, body, .. } => {
            expr_contains_invalidating_call_for(range, fname, ipa)
                || expr_contains_invalidating_call_for(body, fname, ipa)
        }
        ExprKind::Interrupt(opt) => opt.as_ref().map_or(false, |e| expr_contains_invalidating_call_for(e, fname, ipa)),
        ExprKind::TaggedTemplate { tag, args, .. } => {
            expr_contains_invalidating_call_for(tag, fname, ipa)
                || args.iter().any(|a| expr_contains_invalidating_call_for(a, fname, ipa))
        }
        // Closures: values not executed synchronously; not barriers.
        ExprKind::Lambda { .. } | ExprKind::ClosureLight { .. }
        | ExprKind::ClosureFull(_) | ExprKind::HandlerLit { .. }
        | ExprKind::ProtocolLit { .. } => false,
        ExprKind::IntLit(_) | ExprKind::FloatLit(_) | ExprKind::StrLit(_)
        | ExprKind::BoolLit(_) | ExprKind::UnitLit | ExprKind::CharLit(_)
        | ExprKind::NullPtrLit | ExprKind::Ident(_) | ExprKind::Path(_)
        | ExprKind::SelfAccess => false,
    }
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
/// V7.1 adds optional `ipa` param — IPA-aware barrier detection.
fn rewrite_fn_body_split(
    f: &mut FnDecl,
    ro_cache: &[(String, crate::diag::Span)],
    mut_cache: &[(String, crate::diag::Span)],
    name_map: &HashMap<String, String>,
) {
    rewrite_fn_body_split_with_ipa(f, ro_cache, mut_cache, name_map, None)
}

fn rewrite_fn_body_split_with_ipa(
    f: &mut FnDecl,
    ro_cache: &[(String, crate::diag::Span)],
    mut_cache: &[(String, crate::diag::Span)],
    name_map: &HashMap<String, String>,
    ipa: Option<IpaCtx<'_>>,
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
                if stmt_is_barrier_for_with_ipa(s, fname, ipa) {
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
                    let trailing_barrier = expr_is_barrier_for_with_ipa(t, fname, ipa);
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

// ===== Plan 123.2 (D218): LICM — Loop-Invariant Code Motion =====
//
// Phase invoked BEFORE per-fn ro/mut caching (см. cache_module).
// Walks fn body recursively; for each loop (For/While/Loop/WhileLet),
// detects @<F> reads that are invariant w.r.t. loop iteration —
// hoists `ro _at_<F>_loop = @<F>` immediately before the loop in
// the enclosing Block.
//
// ParallelFor — skipped (concurrent body, aliasing safety).
//
// Eligibility per field F:
// - reads inside loop body ≥ cfg.licm_threshold (default 2).
// - NOT written in body (Assign or compound on @F anywhere).
// - NOT captured by any closure in body.
// - Body does NOT contain Spawn / Supervised / Detach / Blocking.
// - For mut field: body does NOT contain any Call (V2 conservative).
// - For ro field: calls in body are OK (frozen — no aliasing).
//
// Naming: `_at_<field>_loop` (distinct from Plan 123.1 `_at_<field>`).
// Collision avoidance: numeric suffix `_<N>`.

/// Per-fn LICM entry point.
fn licm_fn(f: &mut FnDecl, reg: &FieldRegistry, cfg: &FieldCacheConfig) {
    licm_fn_impl(f, reg, cfg, None)
}

/// Plan 123 V7.2 (2026-06-02): explicit IPA threading — replaces the
/// V7.1 thread-local plumbing. `ipa` flows from `cache_module` through
/// every recursive descent in lieu of LICM_WRITE_SETS.with(...) snapshot.
fn licm_fn_impl(
    f: &mut FnDecl,
    reg: &FieldRegistry,
    cfg: &FieldCacheConfig,
    ipa: Option<IpaCtx<'_>>,
) {
    let Some(recv) = &f.receiver else { return };
    if recv.kind == ReceiverKind::Static {
        return;
    }
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
    if f.is_external {
        return;
    }

    // Pre-collect existing locals для collision avoidance.
    let mut local_names: HashSet<String> = HashSet::new();
    for p in &f.params {
        local_names.insert(p.name.clone());
    }
    collect_local_names_fn(f, &mut local_names);

    let mut hoist_count = 0usize;

    match &mut f.body {
        FnBody::Block(b) => {
            licm_block(b, fields, cfg, &mut local_names, &mut hoist_count, ipa);
        }
        FnBody::Expr(e) => {
            // Body is Expr; recurse into it. If the entire body is a
            // loop (rare), we coerce to Block first so hoist can be
            // inserted before it.
            let span = e.span;
            // Check if expr itself is a loop.
            if matches!(e.kind, ExprKind::For { .. } | ExprKind::While { .. }
                       | ExprKind::Loop { .. } | ExprKind::WhileLet { .. }) {
                // Coerce FnBody::Expr → Block-with-trailing.
                let body_expr = match std::mem::replace(&mut f.body, FnBody::External) {
                    FnBody::Expr(e) => e,
                    _ => unreachable!(),
                };
                let block = Block {
                    stmts: Vec::new(),
                    trailing: Some(Box::new(body_expr)),
                    span,
                };
                f.body = FnBody::Block(block);
                if let FnBody::Block(b) = &mut f.body {
                    licm_block(b, fields, cfg, &mut local_names, &mut hoist_count, ipa);
                }
            } else {
                licm_expr(e, fields, cfg, &mut local_names, &mut hoist_count, ipa);
            }
        }
        FnBody::External => {}
    }
}

/// LICM walk for a Block — process inner loops and trailing.
fn licm_block(
    b: &mut Block,
    fields: &HashMap<String, FieldKind>,
    cfg: &FieldCacheConfig,
    local_names: &mut HashSet<String>,
    hoist_count: &mut usize,
    ipa: Option<IpaCtx<'_>>,
) {
    // Phase A: recurse into each stmt first (handles nested loops
    // в inner blocks DFS-postorder — inner loops processed before
    // we hoist for outer). Then rebuild stmts vec, inserting hoists
    // before any stmt that contains a top-level loop expression.
    let old_stmts = std::mem::take(&mut b.stmts);
    let mut new_stmts: Vec<Stmt> = Vec::with_capacity(old_stmts.len() + 4);

    for mut s in old_stmts {
        // First recurse for nested loops в this stmt's sub-blocks.
        licm_stmt(&mut s, fields, cfg, local_names, hoist_count, ipa);

        // If this stmt IS a top-level loop expression, process LICM
        // for it и insert hoists before.
        if let Stmt::Expr(loop_expr) = &mut s {
            if let Some(body_ref) = expr_as_loop_body_mut(loop_expr) {
                process_loop(body_ref, fields, cfg, local_names, hoist_count, &mut new_stmts, ipa);
            }
        }
        new_stmts.push(s);
    }

    b.stmts = new_stmts;

    // Phase B: handle trailing — recurse, then if trailing IS a loop,
    // hoist as last stmts of stmts.
    if let Some(t) = &mut b.trailing {
        licm_expr(t, fields, cfg, local_names, hoist_count, ipa);
        if let Some(body_ref) = expr_as_loop_body_mut(t) {
            process_loop(body_ref, fields, cfg, local_names, hoist_count, &mut b.stmts, ipa);
        }
    }
}

/// Compute eligible fields для hoisting и rewrite loop body.
/// Emit hoist let statements into `out_stmts`.
fn process_loop(
    body: &mut Block,
    fields: &HashMap<String, FieldKind>,
    cfg: &FieldCacheConfig,
    local_names: &mut HashSet<String>,
    hoist_count: &mut usize,
    out_stmts: &mut Vec<Stmt>,
    ipa: Option<IpaCtx<'_>>,
) {
    let eligible = collect_loop_eligible_fields(body, fields, cfg, ipa);
    // Bound by max-per-loop AND remaining max_per_fn quota.
    let remaining = cfg.max_per_fn.saturating_sub(*hoist_count);
    let take = eligible.len().min(cfg.licm_max_per_loop).min(remaining);
    for (fname, span) in eligible.into_iter().take(take) {
        // Generate collision-safe cache local name.
        let base = format!("_at_{}_loop", fname);
        let mut chosen = base.clone();
        let mut suffix = 0usize;
        while local_names.contains(&chosen) {
            suffix += 1;
            chosen = format!("{}_{}", base, suffix);
        }
        local_names.insert(chosen.clone());

        // Emit hoist let — `ro _at_<F>_loop = @<F>`.
        let hoist = make_hoist_let(&chosen, &fname, span);
        out_stmts.push(hoist);

        // Rewrite reads of @F inside loop body.
        let replace_map: HashMap<String, String> =
            std::iter::once((fname.clone(), chosen.clone())).collect();
        rewrite_block(body, &replace_map);

        *hoist_count += 1;
    }
}

/// Helper: build `ro _at_<F>_loop = @<F>` Stmt::Let.
fn make_hoist_let(local_name: &str, fname: &str, span: crate::diag::Span) -> Stmt {
    let access = Expr {
        kind: ExprKind::Member {
            obj: Box::new(Expr {
                kind: ExprKind::SelfAccess,
                span,
            }),
            name: fname.to_string(),
        },
        span,
    };
    Stmt::Let(LetDecl {
        mutable: false,
        pattern: Pattern::Ident {
            name: local_name.to_string(),
            span,
            is_mut: false,
        },
        ty: None,
        value: access,
        span,
        is_ghost: false,
        consume: false,
    })
}

/// Match expression to a mutable loop body Block reference.
fn expr_as_loop_body_mut(e: &mut Expr) -> Option<&mut Block> {
    match &mut e.kind {
        ExprKind::For { body, .. } => Some(body),
        ExprKind::While { body, .. } => Some(body),
        ExprKind::Loop { body, .. } => Some(body),
        ExprKind::WhileLet { body, .. } => Some(body),
        _ => None,
    }
}

/// Recurse LICM into stmt's child blocks.
fn licm_stmt(
    s: &mut Stmt,
    fields: &HashMap<String, FieldKind>,
    cfg: &FieldCacheConfig,
    local_names: &mut HashSet<String>,
    hoist_count: &mut usize,
    ipa: Option<IpaCtx<'_>>,
) {
    match s {
        Stmt::Let(d) => licm_expr(&mut d.value, fields, cfg, local_names, hoist_count, ipa),
        Stmt::Const(d) => licm_expr(&mut d.value, fields, cfg, local_names, hoist_count, ipa),
        Stmt::Expr(e) => licm_expr(e, fields, cfg, local_names, hoist_count, ipa),
        Stmt::Assign { target, value, .. } => {
            licm_expr(target, fields, cfg, local_names, hoist_count, ipa);
            licm_expr(value, fields, cfg, local_names, hoist_count, ipa);
        }
        Stmt::Return { value, .. } => {
            if let Some(v) = value {
                licm_expr(v, fields, cfg, local_names, hoist_count, ipa);
            }
        }
        Stmt::Throw { value, .. } => {
            licm_expr(value, fields, cfg, local_names, hoist_count, ipa);
        }
        Stmt::Defer { body, .. }
        | Stmt::ErrDefer { body, .. }
        | Stmt::OkDefer { body, .. }
        | Stmt::DeferWithResult { body, .. } => {
            licm_expr(body, fields, cfg, local_names, hoist_count, ipa);
        }
        Stmt::ConsumeScope { init, body, .. } => {
            licm_expr(init, fields, cfg, local_names, hoist_count, ipa);
            licm_block(body, fields, cfg, local_names, hoist_count, ipa);
        }
        Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => {
            licm_expr(expr, fields, cfg, local_names, hoist_count, ipa);
        }
        Stmt::Break(_) | Stmt::Continue(_)
        | Stmt::Apply { .. } | Stmt::Calc { .. } | Stmt::Reveal { .. } => {}
    }
}

/// Recurse LICM into all Block-containing expression children.
/// Note: для loops в expression position (as opposed to top-level
/// stmts), we recurse into body but DON'T emit hoist there — hoist
/// requires a Block.stmts list to insert into. Loops в trailing or
/// в conditional expressions get hoisted в их enclosing Block via
/// the licm_block walker.
fn licm_expr(
    e: &mut Expr,
    fields: &HashMap<String, FieldKind>,
    cfg: &FieldCacheConfig,
    local_names: &mut HashSet<String>,
    hoist_count: &mut usize,
    ipa: Option<IpaCtx<'_>>,
) {
    match &mut e.kind {
        ExprKind::Block(b) => licm_block(b, fields, cfg, local_names, hoist_count, ipa),
        ExprKind::If { cond, then, else_ } => {
            licm_expr(cond, fields, cfg, local_names, hoist_count, ipa);
            licm_block(then, fields, cfg, local_names, hoist_count, ipa);
            if let Some(eb) = else_ {
                match eb {
                    ElseBranch::Block(b) => licm_block(b, fields, cfg, local_names, hoist_count, ipa),
                    ElseBranch::If(e) => licm_expr(e, fields, cfg, local_names, hoist_count, ipa),
                }
            }
        }
        ExprKind::IfLet { scrutinee, then, else_, .. } => {
            licm_expr(scrutinee, fields, cfg, local_names, hoist_count, ipa);
            licm_block(then, fields, cfg, local_names, hoist_count, ipa);
            if let Some(eb) = else_ {
                match eb {
                    ElseBranch::Block(b) => licm_block(b, fields, cfg, local_names, hoist_count, ipa),
                    ElseBranch::If(e) => licm_expr(e, fields, cfg, local_names, hoist_count, ipa),
                }
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            licm_expr(scrutinee, fields, cfg, local_names, hoist_count, ipa);
            for arm in arms {
                if let Some(g) = &mut arm.guard {
                    licm_expr(g, fields, cfg, local_names, hoist_count, ipa);
                }
                match &mut arm.body {
                    MatchArmBody::Expr(e) => licm_expr(e, fields, cfg, local_names, hoist_count, ipa),
                    MatchArmBody::Block(b) => licm_block(b, fields, cfg, local_names, hoist_count, ipa),
                }
            }
        }
        ExprKind::For { iter, body, .. } => {
            licm_expr(iter, fields, cfg, local_names, hoist_count, ipa);
            licm_block(body, fields, cfg, local_names, hoist_count, ipa);
        }
        ExprKind::ParallelFor { iter, body, .. } => {
            // Concurrent body — skip LICM. Still recurse into iter
            // (could contain nested control flow).
            licm_expr(iter, fields, cfg, local_names, hoist_count, ipa);
            // DON'T recurse into body — body executes concurrently
            // per element; hoisting would change semantics.
            let _ = body;
        }
        ExprKind::While { cond, body, .. } => {
            licm_expr(cond, fields, cfg, local_names, hoist_count, ipa);
            licm_block(body, fields, cfg, local_names, hoist_count, ipa);
        }
        ExprKind::WhileLet { scrutinee, body, .. } => {
            licm_expr(scrutinee, fields, cfg, local_names, hoist_count, ipa);
            licm_block(body, fields, cfg, local_names, hoist_count, ipa);
        }
        ExprKind::Loop { body, .. } => {
            licm_block(body, fields, cfg, local_names, hoist_count, ipa);
        }
        ExprKind::With { bindings, body } => {
            for wb in bindings {
                licm_expr(&mut wb.handler, fields, cfg, local_names, hoist_count, ipa);
            }
            licm_block(body, fields, cfg, local_names, hoist_count, ipa);
        }
        ExprKind::Forbid { body, .. } | ExprKind::Realtime { body, .. } => {
            licm_block(body, fields, cfg, local_names, hoist_count, ipa);
        }
        ExprKind::Supervised { body, cancel } => {
            // Supervised body — concurrent (fibers). Skip LICM on body.
            let _ = body;
            if let Some(c) = cancel {
                licm_expr(c, fields, cfg, local_names, hoist_count, ipa);
            }
        }
        ExprKind::Detach(_) | ExprKind::Blocking(_) | ExprKind::Spawn(_) => {
            // Concurrent or threadpool body — skip.
        }
        ExprKind::Try(e) | ExprKind::Bang(e) | ExprKind::Member { obj: e, .. }
        | ExprKind::TurboFish { base: e, .. } | ExprKind::As(e, _) | ExprKind::Is(e, _)
        | ExprKind::Unary { operand: e, .. } => {
            licm_expr(e, fields, cfg, local_names, hoist_count, ipa);
        }
        ExprKind::Coalesce(a, b) | ExprKind::Binary { left: a, right: b, .. } => {
            licm_expr(a, fields, cfg, local_names, hoist_count, ipa);
            licm_expr(b, fields, cfg, local_names, hoist_count, ipa);
        }
        ExprKind::Index { obj, index } => {
            licm_expr(obj, fields, cfg, local_names, hoist_count, ipa);
            licm_expr(index, fields, cfg, local_names, hoist_count, ipa);
        }
        ExprKind::Call { func, args, trailing } => {
            licm_expr(func, fields, cfg, local_names, hoist_count, ipa);
            for a in args {
                match a {
                    CallArg::Item(e) | CallArg::Spread(e) =>
                        licm_expr(e, fields, cfg, local_names, hoist_count, ipa),
                    CallArg::Named { value, .. } =>
                        licm_expr(value, fields, cfg, local_names, hoist_count, ipa),
                }
            }
            if let Some(t) = trailing {
                match t {
                    Trailing::Block(b) => licm_block(b, fields, cfg, local_names, hoist_count, ipa),
                    Trailing::Fn(sb) => match &mut sb.body {
                        FnBody::Block(b) => licm_block(b, fields, cfg, local_names, hoist_count, ipa),
                        FnBody::Expr(e) => licm_expr(e, fields, cfg, local_names, hoist_count, ipa),
                        FnBody::External => {}
                    },
                    Trailing::LegacyBlockWithParams(tb) =>
                        licm_block(&mut tb.body, fields, cfg, local_names, hoist_count, ipa),
                }
            }
        }
        ExprKind::InterpolatedStr { parts } => {
            for p in parts {
                if let InterpStrPart::Expr(e) = p {
                    licm_expr(e, fields, cfg, local_names, hoist_count, ipa);
                }
            }
        }
        ExprKind::ArrayLit(elems) => {
            for el in elems {
                match el {
                    ArrayElem::Item(e) | ArrayElem::Spread(e) =>
                        licm_expr(e, fields, cfg, local_names, hoist_count, ipa),
                }
            }
        }
        ExprKind::MapLit { elems, .. } => {
            for el in elems {
                match el {
                    MapElem::Pair(k, v) => {
                        licm_expr(k, fields, cfg, local_names, hoist_count, ipa);
                        licm_expr(v, fields, cfg, local_names, hoist_count, ipa);
                    }
                    MapElem::Spread(e) => licm_expr(e, fields, cfg, local_names, hoist_count, ipa),
                }
            }
        }
        ExprKind::RecordLit { fields: rfields, .. } => {
            for rf in rfields {
                if let Some(v) = &mut rf.value {
                    licm_expr(v, fields, cfg, local_names, hoist_count, ipa);
                }
            }
        }
        ExprKind::TupleLit(elems) => {
            for el in elems {
                licm_expr(el, fields, cfg, local_names, hoist_count, ipa);
            }
        }
        ExprKind::Select { arms } => {
            for arm in arms {
                if let Some(g) = &mut arm.guard {
                    licm_expr(g, fields, cfg, local_names, hoist_count, ipa);
                }
                licm_block(&mut arm.body, fields, cfg, local_names, hoist_count, ipa);
                match &mut arm.op {
                    SelectOp::Recv { chan, .. } => licm_expr(chan, fields, cfg, local_names, hoist_count, ipa),
                    SelectOp::Send { chan, value } => {
                        licm_expr(chan, fields, cfg, local_names, hoist_count, ipa);
                        licm_expr(value, fields, cfg, local_names, hoist_count, ipa);
                    }
                    SelectOp::Default => {}
                }
            }
        }
        ExprKind::Range { start, end, .. } => {
            if let Some(s) = start { licm_expr(s, fields, cfg, local_names, hoist_count, ipa); }
            if let Some(e) = end { licm_expr(e, fields, cfg, local_names, hoist_count, ipa); }
        }
        ExprKind::Forall { range, body, .. } | ExprKind::Exists { range, body, .. } => {
            licm_expr(range, fields, cfg, local_names, hoist_count, ipa);
            licm_expr(body, fields, cfg, local_names, hoist_count, ipa);
        }
        ExprKind::Interrupt(opt) => {
            if let Some(e) = opt { licm_expr(e, fields, cfg, local_names, hoist_count, ipa); }
        }
        ExprKind::Throw(e) => licm_expr(e, fields, cfg, local_names, hoist_count, ipa),
        ExprKind::TaggedTemplate { tag, args, .. } => {
            licm_expr(tag, fields, cfg, local_names, hoist_count, ipa);
            for a in args { licm_expr(a, fields, cfg, local_names, hoist_count, ipa); }
        }
        // Closures — separate scope; don't process LICM inside.
        ExprKind::Lambda { .. } | ExprKind::ClosureLight { .. }
        | ExprKind::ClosureFull(_) | ExprKind::HandlerLit { .. }
        | ExprKind::ProtocolLit { .. } => {}
        ExprKind::IntLit(_) | ExprKind::FloatLit(_) | ExprKind::StrLit(_)
        | ExprKind::BoolLit(_) | ExprKind::UnitLit | ExprKind::CharLit(_)
        | ExprKind::NullPtrLit | ExprKind::Ident(_) | ExprKind::Path(_)
        | ExprKind::SelfAccess => {}
    }
}

/// Compute eligible fields для hoisting from a loop body.
/// Reuses existing helpers: count_field_reads_in_block,
/// block_contains_write_to, block_contains_call, scan_block (closures),
/// и block_contains_spawn (new).
fn collect_loop_eligible_fields(
    body: &Block,
    fields: &HashMap<String, FieldKind>,
    cfg: &FieldCacheConfig,
    ipa: Option<IpaCtx<'_>>,
) -> Vec<(String, crate::diag::Span)> {
    // Spawn / Supervised / Detach / Blocking → skip whole loop.
    if block_contains_spawn(body) {
        return Vec::new();
    }
    // Detect fields captured by closure bodies WITHIN the loop body.
    let mut closure_captured: HashSet<String> = HashSet::new();
    collect_closures_captures_in_block(body, fields, &mut closure_captured);

    // Plan 123 V7.2 (2026-06-02): explicit `ipa` parameter replaces the
    // V7.1 thread-local LICM_WRITE_SETS snapshot. None == V1 conservative
    // "any call = barrier"; Some == frame-aware invalidation via
    // `IpaCtx::call_invalidates_field`.

    let mut result: Vec<(String, crate::diag::Span)> = Vec::new();
    let mut keys: Vec<&String> = fields.keys().collect();
    keys.sort();
    for fname in keys {
        let count = count_field_reads_in_block(body, fname);
        if count < cfg.licm_threshold {
            continue;
        }
        if block_contains_write_to(body, fname) {
            continue;
        }
        if closure_captured.contains(fname) {
            continue;
        }
        let kind = match fields.get(fname) {
            Some(k) => *k,
            None => continue,
        };
        // Mut field — check call barrier per IPA (если context set'нут).
        if matches!(kind, FieldKind::Mut) {
            let body_invalidates_field = match ipa {
                Some(ipa) => block_contains_invalidating_call_for(body, fname, Some(ipa)),
                None => block_contains_call(body), // V1 conservative.
            };
            if body_invalidates_field {
                continue;
            }
        }
        let span = first_field_span_in_block(body, fname).unwrap_or(body.span);
        result.push((fname.clone(), span));
    }
    result
}

/// Walk block looking for nested closure expressions. For each closure
/// body encountered, scan its body using scan_expr/scan_block (which
/// adds all @F references). NB: this does NOT add @F references that
/// appear OUTSIDE closures.
fn collect_closures_captures_in_block(
    b: &Block,
    fields: &HashMap<String, FieldKind>,
    out: &mut HashSet<String>,
) {
    for s in &b.stmts {
        collect_closures_captures_in_stmt(s, fields, out);
    }
    if let Some(t) = &b.trailing {
        collect_closures_captures_in_expr(t, fields, out);
    }
}

fn collect_closures_captures_in_stmt(
    s: &Stmt,
    fields: &HashMap<String, FieldKind>,
    out: &mut HashSet<String>,
) {
    match s {
        Stmt::Let(d) => collect_closures_captures_in_expr(&d.value, fields, out),
        Stmt::Const(d) => collect_closures_captures_in_expr(&d.value, fields, out),
        Stmt::Expr(e) => collect_closures_captures_in_expr(e, fields, out),
        Stmt::Assign { target, value, .. } => {
            collect_closures_captures_in_expr(target, fields, out);
            collect_closures_captures_in_expr(value, fields, out);
        }
        Stmt::Return { value, .. } => {
            if let Some(v) = value {
                collect_closures_captures_in_expr(v, fields, out);
            }
        }
        Stmt::Throw { value, .. } => {
            collect_closures_captures_in_expr(value, fields, out);
        }
        Stmt::Defer { body, .. } | Stmt::ErrDefer { body, .. }
        | Stmt::OkDefer { body, .. } | Stmt::DeferWithResult { body, .. } => {
            collect_closures_captures_in_expr(body, fields, out);
        }
        Stmt::ConsumeScope { init, body, .. } => {
            collect_closures_captures_in_expr(init, fields, out);
            collect_closures_captures_in_block(body, fields, out);
        }
        Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => {
            collect_closures_captures_in_expr(expr, fields, out);
        }
        Stmt::Break(_) | Stmt::Continue(_)
        | Stmt::Apply { .. } | Stmt::Calc { .. } | Stmt::Reveal { .. } => {}
    }
}

fn collect_closures_captures_in_expr(
    e: &Expr,
    fields: &HashMap<String, FieldKind>,
    out: &mut HashSet<String>,
) {
    // Found a closure — scan its body with the capture scanner.
    match &e.kind {
        ExprKind::Lambda { body, .. } => {
            scan_expr(body, fields, out);
            return;
        }
        ExprKind::ClosureLight { body, .. } => {
            match body {
                ClosureBody::Expr(e) => scan_expr(e, fields, out),
                ClosureBody::Block(b) => scan_block(b, fields, out),
            }
            return;
        }
        ExprKind::ClosureFull(sb) => {
            match &sb.body {
                FnBody::Expr(e) => scan_expr(e, fields, out),
                FnBody::Block(b) => scan_block(b, fields, out),
                FnBody::External => {}
            }
            return;
        }
        ExprKind::HandlerLit { methods, .. } | ExprKind::ProtocolLit { methods, .. } => {
            for m in methods {
                match &m.body {
                    HandlerMethodBody::Expr(e) => scan_expr(e, fields, out),
                    HandlerMethodBody::Block(b) => scan_block(b, fields, out),
                }
            }
            return;
        }
        _ => {}
    }
    // No closure here — recurse into sub-expressions / sub-blocks
    // looking for nested closures.
    match &e.kind {
        ExprKind::Block(b) => collect_closures_captures_in_block(b, fields, out),
        ExprKind::If { cond, then, else_ } => {
            collect_closures_captures_in_expr(cond, fields, out);
            collect_closures_captures_in_block(then, fields, out);
            if let Some(eb) = else_ {
                match eb {
                    ElseBranch::Block(b) => collect_closures_captures_in_block(b, fields, out),
                    ElseBranch::If(e) => collect_closures_captures_in_expr(e, fields, out),
                }
            }
        }
        ExprKind::IfLet { scrutinee, then, else_, .. } => {
            collect_closures_captures_in_expr(scrutinee, fields, out);
            collect_closures_captures_in_block(then, fields, out);
            if let Some(eb) = else_ {
                match eb {
                    ElseBranch::Block(b) => collect_closures_captures_in_block(b, fields, out),
                    ElseBranch::If(e) => collect_closures_captures_in_expr(e, fields, out),
                }
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            collect_closures_captures_in_expr(scrutinee, fields, out);
            for arm in arms {
                if let Some(g) = &arm.guard {
                    collect_closures_captures_in_expr(g, fields, out);
                }
                match &arm.body {
                    MatchArmBody::Expr(e) => collect_closures_captures_in_expr(e, fields, out),
                    MatchArmBody::Block(b) => collect_closures_captures_in_block(b, fields, out),
                }
            }
        }
        ExprKind::For { iter, body, .. } | ExprKind::ParallelFor { iter, body, .. } => {
            collect_closures_captures_in_expr(iter, fields, out);
            collect_closures_captures_in_block(body, fields, out);
        }
        ExprKind::While { cond, body, .. } => {
            collect_closures_captures_in_expr(cond, fields, out);
            collect_closures_captures_in_block(body, fields, out);
        }
        ExprKind::WhileLet { scrutinee, body, .. } => {
            collect_closures_captures_in_expr(scrutinee, fields, out);
            collect_closures_captures_in_block(body, fields, out);
        }
        ExprKind::Loop { body, .. } => collect_closures_captures_in_block(body, fields, out),
        ExprKind::With { bindings, body } => {
            for wb in bindings {
                collect_closures_captures_in_expr(&wb.handler, fields, out);
            }
            collect_closures_captures_in_block(body, fields, out);
        }
        ExprKind::Forbid { body, .. } | ExprKind::Realtime { body, .. }
        | ExprKind::Detach(body) | ExprKind::Blocking(body) => {
            collect_closures_captures_in_block(body, fields, out);
        }
        ExprKind::Supervised { body, cancel } => {
            collect_closures_captures_in_block(body, fields, out);
            if let Some(c) = cancel { collect_closures_captures_in_expr(c, fields, out); }
        }
        ExprKind::Spawn(e) | ExprKind::Throw(e) | ExprKind::Try(e) | ExprKind::Bang(e)
        | ExprKind::Member { obj: e, .. } | ExprKind::TurboFish { base: e, .. }
        | ExprKind::As(e, _) | ExprKind::Is(e, _) | ExprKind::Unary { operand: e, .. } => {
            collect_closures_captures_in_expr(e, fields, out);
        }
        ExprKind::Coalesce(a, b) | ExprKind::Binary { left: a, right: b, .. } => {
            collect_closures_captures_in_expr(a, fields, out);
            collect_closures_captures_in_expr(b, fields, out);
        }
        ExprKind::Index { obj, index } => {
            collect_closures_captures_in_expr(obj, fields, out);
            collect_closures_captures_in_expr(index, fields, out);
        }
        ExprKind::Call { func, args, trailing } => {
            collect_closures_captures_in_expr(func, fields, out);
            for a in args {
                collect_closures_captures_in_expr(a.expr(), fields, out);
            }
            if let Some(t) = trailing {
                match t {
                    Trailing::Block(b) => collect_closures_captures_in_block(b, fields, out),
                    Trailing::Fn(sb) => match &sb.body {
                        FnBody::Block(b) => collect_closures_captures_in_block(b, fields, out),
                        FnBody::Expr(e) => collect_closures_captures_in_expr(e, fields, out),
                        FnBody::External => {}
                    },
                    Trailing::LegacyBlockWithParams(tb) =>
                        collect_closures_captures_in_block(&tb.body, fields, out),
                }
            }
        }
        ExprKind::InterpolatedStr { parts } => {
            for p in parts {
                if let InterpStrPart::Expr(e) = p {
                    collect_closures_captures_in_expr(e, fields, out);
                }
            }
        }
        ExprKind::ArrayLit(elems) => {
            for el in elems {
                match el {
                    ArrayElem::Item(e) | ArrayElem::Spread(e) =>
                        collect_closures_captures_in_expr(e, fields, out),
                }
            }
        }
        ExprKind::MapLit { elems, .. } => {
            for el in elems {
                match el {
                    MapElem::Pair(k, v) => {
                        collect_closures_captures_in_expr(k, fields, out);
                        collect_closures_captures_in_expr(v, fields, out);
                    }
                    MapElem::Spread(e) => collect_closures_captures_in_expr(e, fields, out),
                }
            }
        }
        ExprKind::RecordLit { fields: rfields, .. } => {
            for rf in rfields {
                if let Some(v) = &rf.value {
                    collect_closures_captures_in_expr(v, fields, out);
                }
            }
        }
        ExprKind::TupleLit(elems) => {
            for el in elems {
                collect_closures_captures_in_expr(el, fields, out);
            }
        }
        ExprKind::Select { arms } => {
            for arm in arms {
                if let Some(g) = &arm.guard {
                    collect_closures_captures_in_expr(g, fields, out);
                }
                collect_closures_captures_in_block(&arm.body, fields, out);
                match &arm.op {
                    SelectOp::Recv { chan, .. } => collect_closures_captures_in_expr(chan, fields, out),
                    SelectOp::Send { chan, value } => {
                        collect_closures_captures_in_expr(chan, fields, out);
                        collect_closures_captures_in_expr(value, fields, out);
                    }
                    SelectOp::Default => {}
                }
            }
        }
        ExprKind::Range { start, end, .. } => {
            if let Some(s) = start { collect_closures_captures_in_expr(s, fields, out); }
            if let Some(e) = end { collect_closures_captures_in_expr(e, fields, out); }
        }
        ExprKind::Forall { range, body, .. } | ExprKind::Exists { range, body, .. } => {
            collect_closures_captures_in_expr(range, fields, out);
            collect_closures_captures_in_expr(body, fields, out);
        }
        ExprKind::Interrupt(opt) => {
            if let Some(e) = opt { collect_closures_captures_in_expr(e, fields, out); }
        }
        ExprKind::TaggedTemplate { tag, args, .. } => {
            collect_closures_captures_in_expr(tag, fields, out);
            for a in args { collect_closures_captures_in_expr(a, fields, out); }
        }
        // Leaves.
        _ => {}
    }
}

/// Block contains Spawn/Supervised/Detach/Blocking expr.
fn block_contains_spawn(b: &Block) -> bool {
    b.stmts.iter().any(stmt_contains_spawn)
        || b.trailing.as_ref().map_or(false, |t| expr_contains_spawn(t))
}

fn stmt_contains_spawn(s: &Stmt) -> bool {
    match s {
        Stmt::Let(d) => expr_contains_spawn(&d.value),
        Stmt::Const(d) => expr_contains_spawn(&d.value),
        Stmt::Expr(e) => expr_contains_spawn(e),
        Stmt::Assign { target, value, .. } => {
            expr_contains_spawn(target) || expr_contains_spawn(value)
        }
        Stmt::Return { value, .. } => value.as_ref().map_or(false, |v| expr_contains_spawn(v)),
        Stmt::Throw { value, .. } => expr_contains_spawn(value),
        Stmt::Defer { body, .. } | Stmt::ErrDefer { body, .. }
        | Stmt::OkDefer { body, .. } | Stmt::DeferWithResult { body, .. } => {
            expr_contains_spawn(body)
        }
        Stmt::ConsumeScope { init, body, .. } => {
            expr_contains_spawn(init) || block_contains_spawn(body)
        }
        Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => expr_contains_spawn(expr),
        Stmt::Break(_) | Stmt::Continue(_)
        | Stmt::Apply { .. } | Stmt::Calc { .. } | Stmt::Reveal { .. } => false,
    }
}

fn expr_contains_spawn(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Spawn(_) | ExprKind::Supervised { .. }
        | ExprKind::Detach(_) | ExprKind::Blocking(_)
        | ExprKind::ParallelFor { .. } => true,
        ExprKind::Block(b) => block_contains_spawn(b),
        ExprKind::If { cond, then, else_ } => {
            expr_contains_spawn(cond) || block_contains_spawn(then)
                || else_.as_ref().map_or(false, |eb| match eb {
                    ElseBranch::Block(b) => block_contains_spawn(b),
                    ElseBranch::If(e) => expr_contains_spawn(e),
                })
        }
        ExprKind::IfLet { scrutinee, then, else_, .. } => {
            expr_contains_spawn(scrutinee) || block_contains_spawn(then)
                || else_.as_ref().map_or(false, |eb| match eb {
                    ElseBranch::Block(b) => block_contains_spawn(b),
                    ElseBranch::If(e) => expr_contains_spawn(e),
                })
        }
        ExprKind::Match { scrutinee, arms } => {
            expr_contains_spawn(scrutinee) || arms.iter().any(|arm| {
                arm.guard.as_ref().map_or(false, |g| expr_contains_spawn(g))
                    || match &arm.body {
                        MatchArmBody::Expr(e) => expr_contains_spawn(e),
                        MatchArmBody::Block(b) => block_contains_spawn(b),
                    }
            })
        }
        ExprKind::For { iter, body, .. } => {
            expr_contains_spawn(iter) || block_contains_spawn(body)
        }
        ExprKind::While { cond, body, .. } => {
            expr_contains_spawn(cond) || block_contains_spawn(body)
        }
        ExprKind::WhileLet { scrutinee, body, .. } => {
            expr_contains_spawn(scrutinee) || block_contains_spawn(body)
        }
        ExprKind::Loop { body, .. } => block_contains_spawn(body),
        ExprKind::With { bindings, body } => {
            bindings.iter().any(|wb| expr_contains_spawn(&wb.handler))
                || block_contains_spawn(body)
        }
        ExprKind::Forbid { body, .. } | ExprKind::Realtime { body, .. } => {
            block_contains_spawn(body)
        }
        ExprKind::Throw(e) | ExprKind::Spawn(e) => expr_contains_spawn(e),
        ExprKind::Try(e) | ExprKind::Bang(e) | ExprKind::Member { obj: e, .. }
        | ExprKind::TurboFish { base: e, .. } | ExprKind::As(e, _) | ExprKind::Is(e, _)
        | ExprKind::Unary { operand: e, .. } => expr_contains_spawn(e),
        ExprKind::Coalesce(a, b) | ExprKind::Binary { left: a, right: b, .. } => {
            expr_contains_spawn(a) || expr_contains_spawn(b)
        }
        ExprKind::Index { obj, index } => expr_contains_spawn(obj) || expr_contains_spawn(index),
        ExprKind::Call { func, args, trailing } => {
            expr_contains_spawn(func)
                || args.iter().any(|a| expr_contains_spawn(a.expr()))
                || trailing.as_ref().map_or(false, |t| match t {
                    Trailing::Block(b) => block_contains_spawn(b),
                    Trailing::Fn(sb) => match &sb.body {
                        FnBody::Block(b) => block_contains_spawn(b),
                        FnBody::Expr(e) => expr_contains_spawn(e),
                        FnBody::External => false,
                    },
                    Trailing::LegacyBlockWithParams(tb) => block_contains_spawn(&tb.body),
                })
        }
        ExprKind::ArrayLit(elems) => elems.iter().any(|el| match el {
            ArrayElem::Item(e) | ArrayElem::Spread(e) => expr_contains_spawn(e),
        }),
        ExprKind::MapLit { elems, .. } => elems.iter().any(|el| match el {
            MapElem::Pair(k, v) => expr_contains_spawn(k) || expr_contains_spawn(v),
            MapElem::Spread(e) => expr_contains_spawn(e),
        }),
        ExprKind::RecordLit { fields: rfields, .. } => rfields.iter().any(|rf| {
            rf.value.as_ref().map_or(false, |v| expr_contains_spawn(v))
        }),
        ExprKind::TupleLit(elems) => elems.iter().any(|el| expr_contains_spawn(el)),
        ExprKind::InterpolatedStr { parts } => parts.iter().any(|p| {
            if let InterpStrPart::Expr(e) = p { expr_contains_spawn(e) } else { false }
        }),
        ExprKind::Select { arms } => arms.iter().any(|arm| {
            block_contains_spawn(&arm.body)
                || arm.guard.as_ref().map_or(false, |g| expr_contains_spawn(g))
                || match &arm.op {
                    SelectOp::Recv { chan, .. } => expr_contains_spawn(chan),
                    SelectOp::Send { chan, value } => expr_contains_spawn(chan) || expr_contains_spawn(value),
                    SelectOp::Default => false,
                }
        }),
        ExprKind::Range { start, end, .. } => {
            start.as_ref().map_or(false, |s| expr_contains_spawn(s))
                || end.as_ref().map_or(false, |e| expr_contains_spawn(e))
        }
        ExprKind::Forall { range, body, .. } | ExprKind::Exists { range, body, .. } => {
            expr_contains_spawn(range) || expr_contains_spawn(body)
        }
        ExprKind::Interrupt(opt) => opt.as_ref().map_or(false, |e| expr_contains_spawn(e)),
        ExprKind::TaggedTemplate { tag, args, .. } => {
            expr_contains_spawn(tag) || args.iter().any(expr_contains_spawn)
        }
        // Closures — values, not synchronous execution. Don't propagate.
        ExprKind::Lambda { .. } | ExprKind::ClosureLight { .. }
        | ExprKind::ClosureFull(_) | ExprKind::HandlerLit { .. }
        | ExprKind::ProtocolLit { .. } => false,
        ExprKind::IntLit(_) | ExprKind::FloatLit(_) | ExprKind::StrLit(_)
        | ExprKind::BoolLit(_) | ExprKind::UnitLit | ExprKind::CharLit(_)
        | ExprKind::NullPtrLit | ExprKind::Ident(_) | ExprKind::Path(_)
        | ExprKind::SelfAccess => false,
    }
}

/// Find first span of @F access in block (для debug-info на hoisted let).
fn first_field_span_in_block(b: &Block, fname: &str) -> Option<crate::diag::Span> {
    for s in &b.stmts {
        if let Some(sp) = first_field_span_in_stmt(s, fname) {
            return Some(sp);
        }
    }
    b.trailing.as_ref().and_then(|t| first_field_span_in_expr(t, fname))
}

fn first_field_span_in_stmt(s: &Stmt, fname: &str) -> Option<crate::diag::Span> {
    match s {
        Stmt::Let(d) => first_field_span_in_expr(&d.value, fname),
        Stmt::Const(d) => first_field_span_in_expr(&d.value, fname),
        Stmt::Expr(e) => first_field_span_in_expr(e, fname),
        Stmt::Assign { target, value, .. } => {
            first_field_span_in_expr(target, fname).or_else(|| first_field_span_in_expr(value, fname))
        }
        Stmt::Return { value, .. } => value.as_ref().and_then(|v| first_field_span_in_expr(v, fname)),
        Stmt::Throw { value, .. } => first_field_span_in_expr(value, fname),
        Stmt::Defer { body, .. } | Stmt::ErrDefer { body, .. }
        | Stmt::OkDefer { body, .. } | Stmt::DeferWithResult { body, .. } => {
            first_field_span_in_expr(body, fname)
        }
        Stmt::ConsumeScope { init, body, .. } => {
            first_field_span_in_expr(init, fname).or_else(|| first_field_span_in_block(body, fname))
        }
        Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => {
            first_field_span_in_expr(expr, fname)
        }
        Stmt::Break(_) | Stmt::Continue(_)
        | Stmt::Apply { .. } | Stmt::Calc { .. } | Stmt::Reveal { .. } => None,
    }
}

fn first_field_span_in_expr(e: &Expr, fname: &str) -> Option<crate::diag::Span> {
    if let Some(t_fname) = match_self_field(e) {
        if t_fname == fname { return Some(e.span); }
    }
    // Skip into closures — different scope.
    if matches!(&e.kind,
        ExprKind::Lambda { .. } | ExprKind::ClosureLight { .. }
        | ExprKind::ClosureFull(_) | ExprKind::HandlerLit { .. }
        | ExprKind::ProtocolLit { .. })
    {
        return None;
    }
    match &e.kind {
        ExprKind::Block(b) => first_field_span_in_block(b, fname),
        ExprKind::If { cond, then, else_ } => {
            first_field_span_in_expr(cond, fname)
                .or_else(|| first_field_span_in_block(then, fname))
                .or_else(|| else_.as_ref().and_then(|eb| match eb {
                    ElseBranch::Block(b) => first_field_span_in_block(b, fname),
                    ElseBranch::If(e) => first_field_span_in_expr(e, fname),
                }))
        }
        ExprKind::IfLet { scrutinee, then, else_, .. } => {
            first_field_span_in_expr(scrutinee, fname)
                .or_else(|| first_field_span_in_block(then, fname))
                .or_else(|| else_.as_ref().and_then(|eb| match eb {
                    ElseBranch::Block(b) => first_field_span_in_block(b, fname),
                    ElseBranch::If(e) => first_field_span_in_expr(e, fname),
                }))
        }
        ExprKind::Match { scrutinee, arms } => {
            first_field_span_in_expr(scrutinee, fname).or_else(|| {
                arms.iter().find_map(|arm| {
                    arm.guard.as_ref().and_then(|g| first_field_span_in_expr(g, fname))
                        .or_else(|| match &arm.body {
                            MatchArmBody::Expr(e) => first_field_span_in_expr(e, fname),
                            MatchArmBody::Block(b) => first_field_span_in_block(b, fname),
                        })
                })
            })
        }
        ExprKind::For { iter, body, .. } | ExprKind::ParallelFor { iter, body, .. } => {
            first_field_span_in_expr(iter, fname)
                .or_else(|| first_field_span_in_block(body, fname))
        }
        ExprKind::While { cond, body, .. } => {
            first_field_span_in_expr(cond, fname)
                .or_else(|| first_field_span_in_block(body, fname))
        }
        ExprKind::WhileLet { scrutinee, body, .. } => {
            first_field_span_in_expr(scrutinee, fname)
                .or_else(|| first_field_span_in_block(body, fname))
        }
        ExprKind::Loop { body, .. } => first_field_span_in_block(body, fname),
        ExprKind::With { bindings, body } => {
            bindings.iter().find_map(|wb| first_field_span_in_expr(&wb.handler, fname))
                .or_else(|| first_field_span_in_block(body, fname))
        }
        ExprKind::Forbid { body, .. } | ExprKind::Realtime { body, .. }
        | ExprKind::Detach(body) | ExprKind::Blocking(body) => {
            first_field_span_in_block(body, fname)
        }
        ExprKind::Supervised { body, cancel } => {
            first_field_span_in_block(body, fname)
                .or_else(|| cancel.as_ref().and_then(|c| first_field_span_in_expr(c, fname)))
        }
        ExprKind::Spawn(e) | ExprKind::Throw(e) | ExprKind::Try(e) | ExprKind::Bang(e)
        | ExprKind::Member { obj: e, .. } | ExprKind::TurboFish { base: e, .. }
        | ExprKind::As(e, _) | ExprKind::Is(e, _) | ExprKind::Unary { operand: e, .. } => {
            first_field_span_in_expr(e, fname)
        }
        ExprKind::Coalesce(a, b) | ExprKind::Binary { left: a, right: b, .. } => {
            first_field_span_in_expr(a, fname).or_else(|| first_field_span_in_expr(b, fname))
        }
        ExprKind::Index { obj, index } => {
            first_field_span_in_expr(obj, fname).or_else(|| first_field_span_in_expr(index, fname))
        }
        ExprKind::Call { func, args, trailing } => {
            first_field_span_in_expr(func, fname)
                .or_else(|| args.iter().find_map(|a| first_field_span_in_expr(a.expr(), fname)))
                .or_else(|| trailing.as_ref().and_then(|t| match t {
                    Trailing::Block(b) => first_field_span_in_block(b, fname),
                    Trailing::Fn(sb) => match &sb.body {
                        FnBody::Block(b) => first_field_span_in_block(b, fname),
                        FnBody::Expr(e) => first_field_span_in_expr(e, fname),
                        FnBody::External => None,
                    },
                    Trailing::LegacyBlockWithParams(tb) => first_field_span_in_block(&tb.body, fname),
                }))
        }
        ExprKind::ArrayLit(elems) => elems.iter().find_map(|el| match el {
            ArrayElem::Item(e) | ArrayElem::Spread(e) => first_field_span_in_expr(e, fname),
        }),
        ExprKind::MapLit { elems, .. } => elems.iter().find_map(|el| match el {
            MapElem::Pair(k, v) => first_field_span_in_expr(k, fname).or_else(|| first_field_span_in_expr(v, fname)),
            MapElem::Spread(e) => first_field_span_in_expr(e, fname),
        }),
        ExprKind::RecordLit { fields: rfields, .. } => rfields.iter().find_map(|rf| {
            rf.value.as_ref().and_then(|v| first_field_span_in_expr(v, fname))
        }),
        ExprKind::TupleLit(elems) => elems.iter().find_map(|el| first_field_span_in_expr(el, fname)),
        ExprKind::InterpolatedStr { parts } => parts.iter().find_map(|p| {
            if let InterpStrPart::Expr(e) = p { first_field_span_in_expr(e, fname) } else { None }
        }),
        ExprKind::Select { arms } => arms.iter().find_map(|arm| {
            (arm.guard.as_ref().and_then(|g| first_field_span_in_expr(g, fname)))
                .or_else(|| first_field_span_in_block(&arm.body, fname))
                .or_else(|| match &arm.op {
                    SelectOp::Recv { chan, .. } => first_field_span_in_expr(chan, fname),
                    SelectOp::Send { chan, value } => first_field_span_in_expr(chan, fname)
                        .or_else(|| first_field_span_in_expr(value, fname)),
                    SelectOp::Default => None,
                })
        }),
        ExprKind::Range { start, end, .. } => {
            start.as_ref().and_then(|s| first_field_span_in_expr(s, fname))
                .or_else(|| end.as_ref().and_then(|e| first_field_span_in_expr(e, fname)))
        }
        ExprKind::Forall { range, body, .. } | ExprKind::Exists { range, body, .. } => {
            first_field_span_in_expr(range, fname)
                .or_else(|| first_field_span_in_expr(body, fname))
        }
        ExprKind::Interrupt(opt) => opt.as_ref().and_then(|e| first_field_span_in_expr(e, fname)),
        ExprKind::TaggedTemplate { tag, args, .. } => {
            first_field_span_in_expr(tag, fname)
                .or_else(|| args.iter().find_map(|a| first_field_span_in_expr(a, fname)))
        }
        _ => None,
    }
}

// ===== Plan 123.3 (D219): Pure call result caching =====
//
// V3 phase. Caches `@<pure_method>()` results within method body when:
// - pure_method has Purity::Pure (D24 infrastructure)
// - method is args-less (V3 scope; V3.1 adds args-with-literals)
// - method body has no @F write anywhere (conservative invalidation)
// - method body has no closure-capture / no concurrent body
// - call count ≥ cfg.pure_threshold
//
// Cache naming: `_at_<method>_call` (distinct от D217/D218 naming).
//
// Composition с D217+D218:
// - LICM (D218) runs FIRST.
// - Pure-cache (D219) runs SECOND — sees post-LICM AST (hoisted
//   locals don't interfere; @<method>() pattern unaffected by
//   LICM).
// - D217 per-fn cache runs LAST — sees cached pure-call locals
//   as regular Ident references (no @F pattern match).

/// Per-fn V3 pure-call cache entry point.
fn pure_cache_fn(
    f: &mut FnDecl,
    reg: &FieldRegistry,
    pure_methods: &HashSet<(String, String)>,
    cfg: &FieldCacheConfig,
) {
    pure_cache_fn_impl(f, reg, pure_methods, cfg, None)
}

/// Plan 123 V7.2 (2026-06-02): explicit IPA threading for pure-cache.
/// `ipa` carries (recv_type, write_sets, read_sets) into the frame-aware
/// invalidation branch, replacing PURE_IPA_CTX thread-local.
fn pure_cache_fn_impl(
    f: &mut FnDecl,
    reg: &FieldRegistry,
    pure_methods: &HashSet<(String, String)>,
    cfg: &FieldCacheConfig,
    ipa: Option<IpaCtx<'_>>,
) {
    let Some(recv) = &f.receiver else { return };
    if recv.kind == ReceiverKind::Static {
        return;
    }
    let type_name = recv.type_name.clone();
    if reg.skip_types.contains(&type_name) {
        return;
    }
    if f.is_external {
        return;
    }
    // V3 skip if concurrent constructs.
    if body_has_concurrent(&f.body) {
        return;
    }

    // Plan 123 V7.2 (2026-06-02): explicit IPA ctx replaces PURE_IPA_CTX
    // thread-local snapshot. Some == V3.1 frame-aware; None == V3 conservative.
    let pure_ipa: Option<(String, HashMap<(String, String), HashSet<String>>)> =
        ipa.map(|i| (i.recv_type.to_string(), i.read_sets.clone()));

    // Collect body's write-set (which fields are mutated в body).
    let body_writes: HashSet<String> = collect_body_writes(&f.body);

    // Conservative V3 fallback (no IPA): if ANY write → skip all
    // pure caching.
    let use_ipa_frame = pure_ipa.is_some();
    if !use_ipa_frame && !body_writes.is_empty() {
        return;
    }

    // Count `@<method>(args)` calls per canonical (method, args) key.
    let mut counts: HashMap<PureCallKey, usize> = HashMap::new();
    let mut first_spans: HashMap<PureCallKey, crate::diag::Span> = HashMap::new();
    let mut captured_methods: HashSet<PureCallKey> = HashSet::new();
    count_pure_calls_in_body(&f.body, pure_methods, &type_name, &mut counts, &mut first_spans, &mut captured_methods);

    // Collect existing local names for collision avoidance.
    let mut local_names: HashSet<String> = HashSet::new();
    for p in &f.params {
        local_names.insert(p.name.clone());
    }
    collect_local_names_fn(f, &mut local_names);

    // Decide candidates: count >= threshold, not closure-captured.
    let mut keys: Vec<&PureCallKey> = counts.keys().collect();
    keys.sort_by(|a, b| a.method.cmp(&b.method).then(a.args_key.cmp(&b.args_key)));
    let mut to_cache: Vec<(PureCallKey, crate::diag::Span)> = Vec::new();
    let body_span = match &f.body {
        FnBody::Block(b) => b.span,
        FnBody::Expr(e) => e.span,
        FnBody::External => return,
    };
    for k in keys {
        if counts[k] < cfg.pure_threshold {
            continue;
        }
        if captured_methods.contains(k) {
            continue;
        }
        // V3.1 frame-based invalidation: pure method's cache valid iff
        // body writes don't overlap с method's field-read-set.
        if use_ipa_frame {
            if let Some((_, read_sets)) = &pure_ipa {
                let m_key = (type_name.clone(), k.method.clone());
                if let Some(m_reads) = read_sets.get(&m_key) {
                    if body_writes.iter().any(|w| m_reads.contains(w)) {
                        continue;
                    }
                } else {
                    if !body_writes.is_empty() { continue; }
                }
            }
        }
        let span = first_spans.get(k).copied().unwrap_or(body_span);
        to_cache.push((k.clone(), span));
        if to_cache.len() >= cfg.max_per_fn {
            break;
        }
    }

    if to_cache.is_empty() {
        return;
    }

    // Generate collision-safe cache local names. V3.1: include args_key
    // в name to disambiguate same method с different args.
    let mut name_map: HashMap<PureCallKey, String> = HashMap::new();
    for (k, _) in &to_cache {
        let base = if k.args_key.is_empty() {
            format!("_at_{}_call", k.method)
        } else {
            format!("_at_{}{}_call", k.method, k.args_key)
        };
        let mut chosen = base.clone();
        let mut suffix = 0usize;
        while local_names.contains(&chosen) {
            suffix += 1;
            chosen = format!("{}_{}", base, suffix);
        }
        local_names.insert(chosen.clone());
        name_map.insert(k.clone(), chosen);
    }

    // Coerce FnBody::Expr → Block-with-trailing для prepend.
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

    // Capture sample args for each key (for prefix let reconstruction).
    // We need the original ARGS Vec<CallArg> для каждого cached key.
    // Walk body once more to find first call matching each key.
    let mut sample_args: HashMap<PureCallKey, Vec<CallArg>> = HashMap::new();
    capture_sample_args_in_body(&f.body, pure_methods, &type_name, &name_map, &mut sample_args);

    // Rewrite call sites с cache idents (V3.1: match by canonical key).
    if let FnBody::Block(b) = &mut f.body {
        let renames: HashMap<PureCallKey, String> = name_map.iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        rewrite_pure_calls_in_block_v31(b, pure_methods, &type_name, &renames);
    }

    // Prepend cache let statements at body start.
    if let FnBody::Block(b) = &mut f.body {
        let mut prefix: Vec<Stmt> = Vec::with_capacity(to_cache.len());
        for (k, span) in &to_cache {
            let local_name = &name_map[k];
            // Reconstruct `@<method>(<sample_args>)` call expression.
            let args = sample_args.get(k).cloned().unwrap_or_default();
            let call_expr = Expr {
                kind: ExprKind::Call {
                    func: Box::new(Expr {
                        kind: ExprKind::Member {
                            obj: Box::new(Expr {
                                kind: ExprKind::SelfAccess,
                                span: *span,
                            }),
                            name: k.method.clone(),
                        },
                        span: *span,
                    }),
                    args,
                    trailing: None,
                },
                span: *span,
            };
            prefix.push(Stmt::Let(LetDecl {
                mutable: false,
                pattern: Pattern::Ident {
                    name: local_name.clone(),
                    span: *span,
                    is_mut: false,
                },
                ty: None,
                value: call_expr,
                span: *span,
                is_ghost: false,
                consume: false,
            }));
        }
        prefix.append(&mut b.stmts);
        b.stmts = prefix;
    }
}

/// V3.1: walk body, для each cached key save first sample args
/// encountered. Used to reconstruct the prefix let.
fn capture_sample_args_in_body(
    body: &FnBody,
    pure_methods: &HashSet<(String, String)>,
    recv_type: &str,
    name_map: &HashMap<PureCallKey, String>,
    out: &mut HashMap<PureCallKey, Vec<CallArg>>,
) {
    match body {
        FnBody::Block(b) => capture_sample_args_in_block(b, pure_methods, recv_type, name_map, out),
        FnBody::Expr(e) => capture_sample_args_in_expr(e, pure_methods, recv_type, name_map, out),
        FnBody::External => {}
    }
}

fn capture_sample_args_in_block(
    b: &Block,
    pure_methods: &HashSet<(String, String)>,
    recv_type: &str,
    name_map: &HashMap<PureCallKey, String>,
    out: &mut HashMap<PureCallKey, Vec<CallArg>>,
) {
    for s in &b.stmts {
        capture_sample_args_in_stmt(s, pure_methods, recv_type, name_map, out);
    }
    if let Some(t) = &b.trailing {
        capture_sample_args_in_expr(t, pure_methods, recv_type, name_map, out);
    }
}

fn capture_sample_args_in_stmt(
    s: &Stmt,
    pure_methods: &HashSet<(String, String)>,
    recv_type: &str,
    name_map: &HashMap<PureCallKey, String>,
    out: &mut HashMap<PureCallKey, Vec<CallArg>>,
) {
    match s {
        Stmt::Let(d) => capture_sample_args_in_expr(&d.value, pure_methods, recv_type, name_map, out),
        Stmt::Const(d) => capture_sample_args_in_expr(&d.value, pure_methods, recv_type, name_map, out),
        Stmt::Expr(e) => capture_sample_args_in_expr(e, pure_methods, recv_type, name_map, out),
        Stmt::Assign { target, value, .. } => {
            capture_sample_args_in_expr(target, pure_methods, recv_type, name_map, out);
            capture_sample_args_in_expr(value, pure_methods, recv_type, name_map, out);
        }
        Stmt::Return { value, .. } => {
            if let Some(v) = value { capture_sample_args_in_expr(v, pure_methods, recv_type, name_map, out); }
        }
        Stmt::Throw { value, .. } => capture_sample_args_in_expr(value, pure_methods, recv_type, name_map, out),
        Stmt::Defer { body, .. } | Stmt::ErrDefer { body, .. }
        | Stmt::OkDefer { body, .. } | Stmt::DeferWithResult { body, .. } => {
            capture_sample_args_in_expr(body, pure_methods, recv_type, name_map, out);
        }
        Stmt::ConsumeScope { init, body, .. } => {
            capture_sample_args_in_expr(init, pure_methods, recv_type, name_map, out);
            capture_sample_args_in_block(body, pure_methods, recv_type, name_map, out);
        }
        Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => {
            capture_sample_args_in_expr(expr, pure_methods, recv_type, name_map, out);
        }
        Stmt::Break(_) | Stmt::Continue(_)
        | Stmt::Apply { .. } | Stmt::Calc { .. } | Stmt::Reveal { .. } => {}
    }
}

fn capture_sample_args_in_expr(
    e: &Expr,
    pure_methods: &HashSet<(String, String)>,
    recv_type: &str,
    name_map: &HashMap<PureCallKey, String>,
    out: &mut HashMap<PureCallKey, Vec<CallArg>>,
) {
    if let Some(key) = match_self_pure_call(e, pure_methods, recv_type) {
        if name_map.contains_key(&key) && !out.contains_key(&key) {
            if let ExprKind::Call { args, .. } = &e.kind {
                out.insert(key, args.clone());
            }
        }
        return;
    }
    // Recurse children (skip closures).
    match &e.kind {
        ExprKind::Lambda { .. } | ExprKind::ClosureLight { .. }
        | ExprKind::ClosureFull(_) | ExprKind::HandlerLit { .. }
        | ExprKind::ProtocolLit { .. } => {}
        ExprKind::Block(b) => capture_sample_args_in_block(b, pure_methods, recv_type, name_map, out),
        ExprKind::If { cond, then, else_ } => {
            capture_sample_args_in_expr(cond, pure_methods, recv_type, name_map, out);
            capture_sample_args_in_block(then, pure_methods, recv_type, name_map, out);
            if let Some(eb) = else_ {
                match eb {
                    ElseBranch::Block(b) => capture_sample_args_in_block(b, pure_methods, recv_type, name_map, out),
                    ElseBranch::If(e) => capture_sample_args_in_expr(e, pure_methods, recv_type, name_map, out),
                }
            }
        }
        ExprKind::IfLet { scrutinee, then, else_, .. } => {
            capture_sample_args_in_expr(scrutinee, pure_methods, recv_type, name_map, out);
            capture_sample_args_in_block(then, pure_methods, recv_type, name_map, out);
            if let Some(eb) = else_ {
                match eb {
                    ElseBranch::Block(b) => capture_sample_args_in_block(b, pure_methods, recv_type, name_map, out),
                    ElseBranch::If(e) => capture_sample_args_in_expr(e, pure_methods, recv_type, name_map, out),
                }
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            capture_sample_args_in_expr(scrutinee, pure_methods, recv_type, name_map, out);
            for arm in arms {
                if let Some(g) = &arm.guard { capture_sample_args_in_expr(g, pure_methods, recv_type, name_map, out); }
                match &arm.body {
                    MatchArmBody::Expr(e) => capture_sample_args_in_expr(e, pure_methods, recv_type, name_map, out),
                    MatchArmBody::Block(b) => capture_sample_args_in_block(b, pure_methods, recv_type, name_map, out),
                }
            }
        }
        ExprKind::For { iter, body, .. } | ExprKind::ParallelFor { iter, body, .. } => {
            capture_sample_args_in_expr(iter, pure_methods, recv_type, name_map, out);
            capture_sample_args_in_block(body, pure_methods, recv_type, name_map, out);
        }
        ExprKind::While { cond, body, .. } => {
            capture_sample_args_in_expr(cond, pure_methods, recv_type, name_map, out);
            capture_sample_args_in_block(body, pure_methods, recv_type, name_map, out);
        }
        ExprKind::WhileLet { scrutinee, body, .. } => {
            capture_sample_args_in_expr(scrutinee, pure_methods, recv_type, name_map, out);
            capture_sample_args_in_block(body, pure_methods, recv_type, name_map, out);
        }
        ExprKind::Loop { body, .. } => capture_sample_args_in_block(body, pure_methods, recv_type, name_map, out),
        ExprKind::With { bindings, body } => {
            for wb in bindings { capture_sample_args_in_expr(&wb.handler, pure_methods, recv_type, name_map, out); }
            capture_sample_args_in_block(body, pure_methods, recv_type, name_map, out);
        }
        ExprKind::Forbid { body, .. } | ExprKind::Realtime { body, .. }
        | ExprKind::Detach(body) | ExprKind::Blocking(body) => {
            capture_sample_args_in_block(body, pure_methods, recv_type, name_map, out);
        }
        ExprKind::Supervised { body, cancel } => {
            capture_sample_args_in_block(body, pure_methods, recv_type, name_map, out);
            if let Some(c) = cancel { capture_sample_args_in_expr(c, pure_methods, recv_type, name_map, out); }
        }
        ExprKind::Spawn(e) | ExprKind::Throw(e) | ExprKind::Try(e) | ExprKind::Bang(e)
        | ExprKind::Member { obj: e, .. } | ExprKind::TurboFish { base: e, .. }
        | ExprKind::As(e, _) | ExprKind::Is(e, _) | ExprKind::Unary { operand: e, .. } => {
            capture_sample_args_in_expr(e, pure_methods, recv_type, name_map, out);
        }
        ExprKind::Coalesce(a, b) | ExprKind::Binary { left: a, right: b, .. } => {
            capture_sample_args_in_expr(a, pure_methods, recv_type, name_map, out);
            capture_sample_args_in_expr(b, pure_methods, recv_type, name_map, out);
        }
        ExprKind::Index { obj, index } => {
            capture_sample_args_in_expr(obj, pure_methods, recv_type, name_map, out);
            capture_sample_args_in_expr(index, pure_methods, recv_type, name_map, out);
        }
        ExprKind::Call { func, args, trailing } => {
            capture_sample_args_in_expr(func, pure_methods, recv_type, name_map, out);
            for a in args { capture_sample_args_in_expr(a.expr(), pure_methods, recv_type, name_map, out); }
            if let Some(t) = trailing {
                if let Trailing::Block(b) = t { capture_sample_args_in_block(b, pure_methods, recv_type, name_map, out); }
            }
        }
        ExprKind::ArrayLit(elems) => {
            for el in elems {
                match el {
                    ArrayElem::Item(e) | ArrayElem::Spread(e) => capture_sample_args_in_expr(e, pure_methods, recv_type, name_map, out),
                }
            }
        }
        ExprKind::TupleLit(elems) => {
            for el in elems { capture_sample_args_in_expr(el, pure_methods, recv_type, name_map, out); }
        }
        ExprKind::RecordLit { fields: rfields, .. } => {
            for rf in rfields {
                if let Some(v) = &rf.value { capture_sample_args_in_expr(v, pure_methods, recv_type, name_map, out); }
            }
        }
        ExprKind::Range { start, end, .. } => {
            if let Some(s) = start { capture_sample_args_in_expr(s, pure_methods, recv_type, name_map, out); }
            if let Some(e) = end { capture_sample_args_in_expr(e, pure_methods, recv_type, name_map, out); }
        }
        ExprKind::Forall { range, body, .. } | ExprKind::Exists { range, body, .. } => {
            capture_sample_args_in_expr(range, pure_methods, recv_type, name_map, out);
            capture_sample_args_in_expr(body, pure_methods, recv_type, name_map, out);
        }
        _ => {}
    }
}

/// V3.1: rewrite pure-call sites c canonical key matching.
fn rewrite_pure_calls_in_block_v31(
    b: &mut Block,
    pure_methods: &HashSet<(String, String)>,
    recv_type: &str,
    renames: &HashMap<PureCallKey, String>,
) {
    for s in &mut b.stmts {
        rewrite_pure_calls_in_stmt_v31(s, pure_methods, recv_type, renames);
    }
    if let Some(t) = &mut b.trailing {
        rewrite_pure_calls_in_expr_v31(t, pure_methods, recv_type, renames);
    }
}

fn rewrite_pure_calls_in_stmt_v31(
    s: &mut Stmt,
    pure_methods: &HashSet<(String, String)>,
    recv_type: &str,
    renames: &HashMap<PureCallKey, String>,
) {
    match s {
        Stmt::Let(d) => rewrite_pure_calls_in_expr_v31(&mut d.value, pure_methods, recv_type, renames),
        Stmt::Const(d) => rewrite_pure_calls_in_expr_v31(&mut d.value, pure_methods, recv_type, renames),
        Stmt::Expr(e) => rewrite_pure_calls_in_expr_v31(e, pure_methods, recv_type, renames),
        Stmt::Assign { target, value, .. } => {
            rewrite_pure_calls_in_expr_v31(target, pure_methods, recv_type, renames);
            rewrite_pure_calls_in_expr_v31(value, pure_methods, recv_type, renames);
        }
        Stmt::Return { value, .. } => {
            if let Some(v) = value { rewrite_pure_calls_in_expr_v31(v, pure_methods, recv_type, renames); }
        }
        Stmt::Throw { value, .. } => rewrite_pure_calls_in_expr_v31(value, pure_methods, recv_type, renames),
        Stmt::Defer { body, .. } | Stmt::ErrDefer { body, .. }
        | Stmt::OkDefer { body, .. } | Stmt::DeferWithResult { body, .. } => {
            rewrite_pure_calls_in_expr_v31(body, pure_methods, recv_type, renames);
        }
        Stmt::ConsumeScope { init, body, .. } => {
            rewrite_pure_calls_in_expr_v31(init, pure_methods, recv_type, renames);
            rewrite_pure_calls_in_block_v31(body, pure_methods, recv_type, renames);
        }
        Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => {
            rewrite_pure_calls_in_expr_v31(expr, pure_methods, recv_type, renames);
        }
        Stmt::Break(_) | Stmt::Continue(_)
        | Stmt::Apply { .. } | Stmt::Calc { .. } | Stmt::Reveal { .. } => {}
    }
}

fn rewrite_pure_calls_in_expr_v31(
    e: &mut Expr,
    pure_methods: &HashSet<(String, String)>,
    recv_type: &str,
    renames: &HashMap<PureCallKey, String>,
) {
    if let Some(key) = match_self_pure_call(e, pure_methods, recv_type) {
        if let Some(local) = renames.get(&key) {
            e.kind = ExprKind::Ident(local.clone());
            return;
        }
    }
    match &mut e.kind {
        ExprKind::Lambda { .. } | ExprKind::ClosureLight { .. }
        | ExprKind::ClosureFull(_) | ExprKind::HandlerLit { .. }
        | ExprKind::ProtocolLit { .. } => return,
        _ => {}
    }
    match &mut e.kind {
        ExprKind::Block(b) => rewrite_pure_calls_in_block_v31(b, pure_methods, recv_type, renames),
        ExprKind::If { cond, then, else_ } => {
            rewrite_pure_calls_in_expr_v31(cond, pure_methods, recv_type, renames);
            rewrite_pure_calls_in_block_v31(then, pure_methods, recv_type, renames);
            if let Some(eb) = else_ {
                match eb {
                    ElseBranch::Block(b) => rewrite_pure_calls_in_block_v31(b, pure_methods, recv_type, renames),
                    ElseBranch::If(e) => rewrite_pure_calls_in_expr_v31(e, pure_methods, recv_type, renames),
                }
            }
        }
        ExprKind::IfLet { scrutinee, then, else_, .. } => {
            rewrite_pure_calls_in_expr_v31(scrutinee, pure_methods, recv_type, renames);
            rewrite_pure_calls_in_block_v31(then, pure_methods, recv_type, renames);
            if let Some(eb) = else_ {
                match eb {
                    ElseBranch::Block(b) => rewrite_pure_calls_in_block_v31(b, pure_methods, recv_type, renames),
                    ElseBranch::If(e) => rewrite_pure_calls_in_expr_v31(e, pure_methods, recv_type, renames),
                }
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            rewrite_pure_calls_in_expr_v31(scrutinee, pure_methods, recv_type, renames);
            for arm in arms {
                if let Some(g) = &mut arm.guard { rewrite_pure_calls_in_expr_v31(g, pure_methods, recv_type, renames); }
                match &mut arm.body {
                    MatchArmBody::Expr(e) => rewrite_pure_calls_in_expr_v31(e, pure_methods, recv_type, renames),
                    MatchArmBody::Block(b) => rewrite_pure_calls_in_block_v31(b, pure_methods, recv_type, renames),
                }
            }
        }
        ExprKind::For { iter, body, .. } | ExprKind::ParallelFor { iter, body, .. } => {
            rewrite_pure_calls_in_expr_v31(iter, pure_methods, recv_type, renames);
            rewrite_pure_calls_in_block_v31(body, pure_methods, recv_type, renames);
        }
        ExprKind::While { cond, body, .. } => {
            rewrite_pure_calls_in_expr_v31(cond, pure_methods, recv_type, renames);
            rewrite_pure_calls_in_block_v31(body, pure_methods, recv_type, renames);
        }
        ExprKind::WhileLet { scrutinee, body, .. } => {
            rewrite_pure_calls_in_expr_v31(scrutinee, pure_methods, recv_type, renames);
            rewrite_pure_calls_in_block_v31(body, pure_methods, recv_type, renames);
        }
        ExprKind::Loop { body, .. } => rewrite_pure_calls_in_block_v31(body, pure_methods, recv_type, renames),
        ExprKind::With { bindings, body } => {
            for wb in bindings { rewrite_pure_calls_in_expr_v31(&mut wb.handler, pure_methods, recv_type, renames); }
            rewrite_pure_calls_in_block_v31(body, pure_methods, recv_type, renames);
        }
        ExprKind::Forbid { body, .. } | ExprKind::Realtime { body, .. }
        | ExprKind::Detach(body) | ExprKind::Blocking(body) => {
            rewrite_pure_calls_in_block_v31(body, pure_methods, recv_type, renames);
        }
        ExprKind::Supervised { body, cancel } => {
            rewrite_pure_calls_in_block_v31(body, pure_methods, recv_type, renames);
            if let Some(c) = cancel { rewrite_pure_calls_in_expr_v31(c, pure_methods, recv_type, renames); }
        }
        ExprKind::Spawn(e) | ExprKind::Throw(e) | ExprKind::Try(e) | ExprKind::Bang(e)
        | ExprKind::Member { obj: e, .. } | ExprKind::TurboFish { base: e, .. }
        | ExprKind::As(e, _) | ExprKind::Is(e, _) | ExprKind::Unary { operand: e, .. } => {
            rewrite_pure_calls_in_expr_v31(e, pure_methods, recv_type, renames);
        }
        ExprKind::Coalesce(a, b) | ExprKind::Binary { left: a, right: b, .. } => {
            rewrite_pure_calls_in_expr_v31(a, pure_methods, recv_type, renames);
            rewrite_pure_calls_in_expr_v31(b, pure_methods, recv_type, renames);
        }
        ExprKind::Index { obj, index } => {
            rewrite_pure_calls_in_expr_v31(obj, pure_methods, recv_type, renames);
            rewrite_pure_calls_in_expr_v31(index, pure_methods, recv_type, renames);
        }
        ExprKind::Call { func, args, trailing } => {
            rewrite_pure_calls_in_expr_v31(func, pure_methods, recv_type, renames);
            for a in args {
                match a {
                    CallArg::Item(e) | CallArg::Spread(e) => rewrite_pure_calls_in_expr_v31(e, pure_methods, recv_type, renames),
                    CallArg::Named { value, .. } => rewrite_pure_calls_in_expr_v31(value, pure_methods, recv_type, renames),
                }
            }
            if let Some(t) = trailing {
                if let Trailing::Block(b) = t { rewrite_pure_calls_in_block_v31(b, pure_methods, recv_type, renames); }
            }
        }
        ExprKind::ArrayLit(elems) => {
            for el in elems {
                match el {
                    ArrayElem::Item(e) | ArrayElem::Spread(e) => rewrite_pure_calls_in_expr_v31(e, pure_methods, recv_type, renames),
                }
            }
        }
        ExprKind::TupleLit(elems) => {
            for el in elems { rewrite_pure_calls_in_expr_v31(el, pure_methods, recv_type, renames); }
        }
        ExprKind::RecordLit { fields: rfields, .. } => {
            for rf in rfields {
                if let Some(v) = &mut rf.value { rewrite_pure_calls_in_expr_v31(v, pure_methods, recv_type, renames); }
            }
        }
        ExprKind::Range { start, end, .. } => {
            if let Some(s) = start { rewrite_pure_calls_in_expr_v31(s, pure_methods, recv_type, renames); }
            if let Some(e) = end { rewrite_pure_calls_in_expr_v31(e, pure_methods, recv_type, renames); }
        }
        ExprKind::Forall { range, body, .. } | ExprKind::Exists { range, body, .. } => {
            rewrite_pure_calls_in_expr_v31(range, pure_methods, recv_type, renames);
            rewrite_pure_calls_in_expr_v31(body, pure_methods, recv_type, renames);
        }
        _ => {}
    }
}

/// Plan 123.7.1 Ф.3: collect set of @F written в body. Direct writes
/// only (top-level Assign with target Member{SelfAccess, F}).
fn collect_body_writes(body: &FnBody) -> HashSet<String> {
    let mut out: HashSet<String> = HashSet::new();
    match body {
        FnBody::Block(b) => collect_body_writes_block(b, &mut out),
        FnBody::Expr(e) => collect_body_writes_expr(e, &mut out),
        FnBody::External => {}
    }
    out
}

fn collect_body_writes_block(b: &Block, out: &mut HashSet<String>) {
    for s in &b.stmts { collect_body_writes_stmt(s, out); }
    if let Some(t) = &b.trailing { collect_body_writes_expr(t, out); }
}

fn collect_body_writes_stmt(s: &Stmt, out: &mut HashSet<String>) {
    match s {
        Stmt::Assign { target, value, .. } => {
            if let Some(fname) = match_self_field(target) {
                out.insert(fname.to_string());
            } else {
                collect_body_writes_expr(target, out);
            }
            collect_body_writes_expr(value, out);
        }
        Stmt::Let(d) => collect_body_writes_expr(&d.value, out),
        Stmt::Const(d) => collect_body_writes_expr(&d.value, out),
        Stmt::Expr(e) => collect_body_writes_expr(e, out),
        Stmt::Return { value, .. } => {
            if let Some(v) = value { collect_body_writes_expr(v, out); }
        }
        Stmt::Throw { value, .. } => collect_body_writes_expr(value, out),
        Stmt::Defer { body, .. } | Stmt::ErrDefer { body, .. }
        | Stmt::OkDefer { body, .. } | Stmt::DeferWithResult { body, .. } => {
            collect_body_writes_expr(body, out);
        }
        Stmt::ConsumeScope { init, body, .. } => {
            collect_body_writes_expr(init, out);
            collect_body_writes_block(body, out);
        }
        Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => {
            collect_body_writes_expr(expr, out);
        }
        Stmt::Break(_) | Stmt::Continue(_)
        | Stmt::Apply { .. } | Stmt::Calc { .. } | Stmt::Reveal { .. } => {}
    }
}

fn collect_body_writes_expr(e: &Expr, out: &mut HashSet<String>) {
    match &e.kind {
        ExprKind::Block(b) => collect_body_writes_block(b, out),
        ExprKind::If { cond, then, else_ } => {
            collect_body_writes_expr(cond, out);
            collect_body_writes_block(then, out);
            if let Some(eb) = else_ {
                match eb {
                    ElseBranch::Block(b) => collect_body_writes_block(b, out),
                    ElseBranch::If(e) => collect_body_writes_expr(e, out),
                }
            }
        }
        ExprKind::IfLet { scrutinee, then, else_, .. } => {
            collect_body_writes_expr(scrutinee, out);
            collect_body_writes_block(then, out);
            if let Some(eb) = else_ {
                match eb {
                    ElseBranch::Block(b) => collect_body_writes_block(b, out),
                    ElseBranch::If(e) => collect_body_writes_expr(e, out),
                }
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            collect_body_writes_expr(scrutinee, out);
            for arm in arms {
                if let Some(g) = &arm.guard { collect_body_writes_expr(g, out); }
                match &arm.body {
                    MatchArmBody::Expr(e) => collect_body_writes_expr(e, out),
                    MatchArmBody::Block(b) => collect_body_writes_block(b, out),
                }
            }
        }
        ExprKind::For { iter, body, .. } | ExprKind::ParallelFor { iter, body, .. } => {
            collect_body_writes_expr(iter, out);
            collect_body_writes_block(body, out);
        }
        ExprKind::While { cond, body, .. } => {
            collect_body_writes_expr(cond, out);
            collect_body_writes_block(body, out);
        }
        ExprKind::WhileLet { scrutinee, body, .. } => {
            collect_body_writes_expr(scrutinee, out);
            collect_body_writes_block(body, out);
        }
        ExprKind::Loop { body, .. } => collect_body_writes_block(body, out),
        ExprKind::With { bindings, body } => {
            for wb in bindings { collect_body_writes_expr(&wb.handler, out); }
            collect_body_writes_block(body, out);
        }
        ExprKind::Forbid { body, .. } | ExprKind::Realtime { body, .. }
        | ExprKind::Detach(body) | ExprKind::Blocking(body) => {
            collect_body_writes_block(body, out);
        }
        ExprKind::Supervised { body, cancel } => {
            collect_body_writes_block(body, out);
            if let Some(c) = cancel { collect_body_writes_expr(c, out); }
        }
        ExprKind::Spawn(e) | ExprKind::Throw(e) | ExprKind::Try(e) | ExprKind::Bang(e)
        | ExprKind::Member { obj: e, .. } | ExprKind::TurboFish { base: e, .. }
        | ExprKind::As(e, _) | ExprKind::Is(e, _) | ExprKind::Unary { operand: e, .. } => {
            collect_body_writes_expr(e, out);
        }
        ExprKind::Coalesce(a, b) | ExprKind::Binary { left: a, right: b, .. } => {
            collect_body_writes_expr(a, out);
            collect_body_writes_expr(b, out);
        }
        ExprKind::Index { obj, index } => {
            collect_body_writes_expr(obj, out);
            collect_body_writes_expr(index, out);
        }
        ExprKind::Call { func, args, trailing } => {
            collect_body_writes_expr(func, out);
            for a in args { collect_body_writes_expr(a.expr(), out); }
            if let Some(t) = trailing {
                if let Trailing::Block(b) = t { collect_body_writes_block(b, out); }
            }
        }
        ExprKind::ArrayLit(elems) => {
            for el in elems {
                match el {
                    ArrayElem::Item(e) | ArrayElem::Spread(e) => collect_body_writes_expr(e, out),
                }
            }
        }
        ExprKind::TupleLit(elems) => {
            for el in elems { collect_body_writes_expr(el, out); }
        }
        ExprKind::RecordLit { fields: rfields, .. } => {
            for rf in rfields {
                if let Some(v) = &rf.value { collect_body_writes_expr(v, out); }
            }
        }
        ExprKind::MapLit { elems, .. } => {
            for el in elems {
                match el {
                    MapElem::Pair(k, v) => {
                        collect_body_writes_expr(k, out);
                        collect_body_writes_expr(v, out);
                    }
                    MapElem::Spread(e) => collect_body_writes_expr(e, out),
                }
            }
        }
        ExprKind::InterpolatedStr { parts } => {
            for p in parts {
                if let InterpStrPart::Expr(e) = p { collect_body_writes_expr(e, out); }
            }
        }
        ExprKind::Select { arms } => {
            for arm in arms {
                if let Some(g) = &arm.guard { collect_body_writes_expr(g, out); }
                collect_body_writes_block(&arm.body, out);
            }
        }
        ExprKind::Range { start, end, .. } => {
            if let Some(s) = start { collect_body_writes_expr(s, out); }
            if let Some(e) = end { collect_body_writes_expr(e, out); }
        }
        ExprKind::Forall { range, body, .. } | ExprKind::Exists { range, body, .. } => {
            collect_body_writes_expr(range, out);
            collect_body_writes_expr(body, out);
        }
        ExprKind::Interrupt(opt) => {
            if let Some(e) = opt { collect_body_writes_expr(e, out); }
        }
        ExprKind::TaggedTemplate { tag, args, .. } => {
            collect_body_writes_expr(tag, out);
            for a in args { collect_body_writes_expr(a, out); }
        }
        // Closures (separate scope) + literals.
        _ => {}
    }
}

/// True if body contains any top-level Assign with target = `@<F>`.
fn body_has_any_self_field_write(body: &FnBody) -> bool {
    match body {
        FnBody::Block(b) => block_has_any_self_field_write(b),
        FnBody::Expr(e) => expr_has_any_self_field_write(e),
        FnBody::External => false,
    }
}

fn block_has_any_self_field_write(b: &Block) -> bool {
    b.stmts.iter().any(stmt_has_any_self_field_write)
        || b.trailing.as_ref().map_or(false, |t| expr_has_any_self_field_write(t))
}

fn stmt_has_any_self_field_write(s: &Stmt) -> bool {
    match s {
        Stmt::Assign { target, .. } => {
            if match_self_field(target).is_some() {
                return true;
            }
            // Even в nested expression — e.g. `let x = if cond { @F = ... }`.
            // For V3 conservative, just check top-level Assign на target.
            false
        }
        Stmt::Let(d) => expr_has_any_self_field_write(&d.value),
        Stmt::Const(d) => expr_has_any_self_field_write(&d.value),
        Stmt::Expr(e) => expr_has_any_self_field_write(e),
        Stmt::Return { value, .. } => value.as_ref().map_or(false, |v| expr_has_any_self_field_write(v)),
        Stmt::Throw { value, .. } => expr_has_any_self_field_write(value),
        Stmt::Defer { body, .. } | Stmt::ErrDefer { body, .. }
        | Stmt::OkDefer { body, .. } | Stmt::DeferWithResult { body, .. } => {
            expr_has_any_self_field_write(body)
        }
        Stmt::ConsumeScope { init, body, .. } => {
            expr_has_any_self_field_write(init) || block_has_any_self_field_write(body)
        }
        Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => {
            expr_has_any_self_field_write(expr)
        }
        Stmt::Break(_) | Stmt::Continue(_)
        | Stmt::Apply { .. } | Stmt::Calc { .. } | Stmt::Reveal { .. } => false,
    }
}

fn expr_has_any_self_field_write(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Block(b) => block_has_any_self_field_write(b),
        ExprKind::If { cond, then, else_ } => {
            expr_has_any_self_field_write(cond)
                || block_has_any_self_field_write(then)
                || else_.as_ref().map_or(false, |eb| match eb {
                    ElseBranch::Block(b) => block_has_any_self_field_write(b),
                    ElseBranch::If(e) => expr_has_any_self_field_write(e),
                })
        }
        ExprKind::IfLet { scrutinee, then, else_, .. } => {
            expr_has_any_self_field_write(scrutinee)
                || block_has_any_self_field_write(then)
                || else_.as_ref().map_or(false, |eb| match eb {
                    ElseBranch::Block(b) => block_has_any_self_field_write(b),
                    ElseBranch::If(e) => expr_has_any_self_field_write(e),
                })
        }
        ExprKind::Match { scrutinee, arms } => {
            expr_has_any_self_field_write(scrutinee)
                || arms.iter().any(|arm| match &arm.body {
                    MatchArmBody::Expr(e) => expr_has_any_self_field_write(e),
                    MatchArmBody::Block(b) => block_has_any_self_field_write(b),
                })
        }
        ExprKind::For { iter, body, .. } | ExprKind::ParallelFor { iter, body, .. } => {
            expr_has_any_self_field_write(iter) || block_has_any_self_field_write(body)
        }
        ExprKind::While { cond, body, .. } => {
            expr_has_any_self_field_write(cond) || block_has_any_self_field_write(body)
        }
        ExprKind::WhileLet { scrutinee, body, .. } => {
            expr_has_any_self_field_write(scrutinee) || block_has_any_self_field_write(body)
        }
        ExprKind::Loop { body, .. } => block_has_any_self_field_write(body),
        ExprKind::With { body, .. } => block_has_any_self_field_write(body),
        ExprKind::Forbid { body, .. } | ExprKind::Realtime { body, .. }
        | ExprKind::Detach(body) | ExprKind::Blocking(body) => {
            block_has_any_self_field_write(body)
        }
        ExprKind::Supervised { body, .. } => block_has_any_self_field_write(body),
        ExprKind::Spawn(e) | ExprKind::Throw(e) | ExprKind::Try(e) | ExprKind::Bang(e)
        | ExprKind::Member { obj: e, .. } | ExprKind::TurboFish { base: e, .. }
        | ExprKind::As(e, _) | ExprKind::Is(e, _) | ExprKind::Unary { operand: e, .. } => {
            expr_has_any_self_field_write(e)
        }
        ExprKind::Coalesce(a, b) | ExprKind::Binary { left: a, right: b, .. } => {
            expr_has_any_self_field_write(a) || expr_has_any_self_field_write(b)
        }
        ExprKind::Index { obj, index } => {
            expr_has_any_self_field_write(obj) || expr_has_any_self_field_write(index)
        }
        ExprKind::Call { func, args, .. } => {
            expr_has_any_self_field_write(func)
                || args.iter().any(|a| expr_has_any_self_field_write(a.expr()))
        }
        _ => false, // Conservative — other expressions don't have stmts.
    }
}

/// True if body contains Spawn/Supervised/Detach/Blocking/ParallelFor.
fn body_has_concurrent(body: &FnBody) -> bool {
    match body {
        FnBody::Block(b) => block_contains_spawn(b),
        FnBody::Expr(e) => expr_contains_spawn(e),
        FnBody::External => false,
    }
}

/// Walk body and count `@<method>()` calls where (recv_type, method) is
/// в pure_methods. Also track first span per method.
/// Additionally, detect closure-captured calls (treat closure-internal
/// calls как captured → exclude).
fn count_pure_calls_in_body(
    body: &FnBody,
    pure_methods: &HashSet<(String, String)>,
    recv_type: &str,
    counts: &mut HashMap<PureCallKey, usize>,
    first_spans: &mut HashMap<PureCallKey, crate::diag::Span>,
    captured: &mut HashSet<PureCallKey>,
) {
    match body {
        FnBody::Block(b) => count_pure_in_block(b, pure_methods, recv_type, counts, first_spans, captured, false),
        FnBody::Expr(e) => count_pure_in_expr(e, pure_methods, recv_type, counts, first_spans, captured, false),
        FnBody::External => {}
    }
}

fn count_pure_in_block(
    b: &Block,
    pure_methods: &HashSet<(String, String)>,
    recv_type: &str,
    counts: &mut HashMap<PureCallKey, usize>,
    first_spans: &mut HashMap<PureCallKey, crate::diag::Span>,
    captured: &mut HashSet<PureCallKey>,
    in_closure: bool,
) {
    for s in &b.stmts {
        count_pure_in_stmt(s, pure_methods, recv_type, counts, first_spans, captured, in_closure);
    }
    if let Some(t) = &b.trailing {
        count_pure_in_expr(t, pure_methods, recv_type, counts, first_spans, captured, in_closure);
    }
}

fn count_pure_in_stmt(
    s: &Stmt,
    pure_methods: &HashSet<(String, String)>,
    recv_type: &str,
    counts: &mut HashMap<PureCallKey, usize>,
    first_spans: &mut HashMap<PureCallKey, crate::diag::Span>,
    captured: &mut HashSet<PureCallKey>,
    in_closure: bool,
) {
    match s {
        Stmt::Let(d) => count_pure_in_expr(&d.value, pure_methods, recv_type, counts, first_spans, captured, in_closure),
        Stmt::Const(d) => count_pure_in_expr(&d.value, pure_methods, recv_type, counts, first_spans, captured, in_closure),
        Stmt::Expr(e) => count_pure_in_expr(e, pure_methods, recv_type, counts, first_spans, captured, in_closure),
        Stmt::Assign { target, value, .. } => {
            count_pure_in_expr(target, pure_methods, recv_type, counts, first_spans, captured, in_closure);
            count_pure_in_expr(value, pure_methods, recv_type, counts, first_spans, captured, in_closure);
        }
        Stmt::Return { value, .. } => {
            if let Some(v) = value {
                count_pure_in_expr(v, pure_methods, recv_type, counts, first_spans, captured, in_closure);
            }
        }
        Stmt::Throw { value, .. } => count_pure_in_expr(value, pure_methods, recv_type, counts, first_spans, captured, in_closure),
        Stmt::Defer { body, .. } | Stmt::ErrDefer { body, .. }
        | Stmt::OkDefer { body, .. } | Stmt::DeferWithResult { body, .. } => {
            count_pure_in_expr(body, pure_methods, recv_type, counts, first_spans, captured, in_closure);
        }
        Stmt::ConsumeScope { init, body, .. } => {
            count_pure_in_expr(init, pure_methods, recv_type, counts, first_spans, captured, in_closure);
            count_pure_in_block(body, pure_methods, recv_type, counts, first_spans, captured, in_closure);
        }
        Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => {
            count_pure_in_expr(expr, pure_methods, recv_type, counts, first_spans, captured, in_closure);
        }
        Stmt::Break(_) | Stmt::Continue(_)
        | Stmt::Apply { .. } | Stmt::Calc { .. } | Stmt::Reveal { .. } => {}
    }
}

fn count_pure_in_expr(
    e: &Expr,
    pure_methods: &HashSet<(String, String)>,
    recv_type: &str,
    counts: &mut HashMap<PureCallKey, usize>,
    first_spans: &mut HashMap<PureCallKey, crate::diag::Span>,
    captured: &mut HashSet<PureCallKey>,
    in_closure: bool,
) {
    // Detect `@<method>(literal-args)` pattern (V3.1).
    if let Some(key) = match_self_pure_call(e, pure_methods, recv_type) {
        if in_closure {
            captured.insert(key);
        } else {
            *counts.entry(key.clone()).or_insert(0) += 1;
            first_spans.entry(key).or_insert(e.span);
        }
        // Don't recurse into args (literal — no further interesting subexprs).
        return;
    }
    // Recurse, switching in_closure flag when entering closure bodies.
    match &e.kind {
        ExprKind::Lambda { body, .. } => count_pure_in_expr(body, pure_methods, recv_type, counts, first_spans, captured, true),
        ExprKind::ClosureLight { body, .. } => {
            match body {
                ClosureBody::Expr(e) => count_pure_in_expr(e, pure_methods, recv_type, counts, first_spans, captured, true),
                ClosureBody::Block(b) => count_pure_in_block(b, pure_methods, recv_type, counts, first_spans, captured, true),
            }
        }
        ExprKind::ClosureFull(sb) => {
            match &sb.body {
                FnBody::Expr(e) => count_pure_in_expr(e, pure_methods, recv_type, counts, first_spans, captured, true),
                FnBody::Block(b) => count_pure_in_block(b, pure_methods, recv_type, counts, first_spans, captured, true),
                FnBody::External => {}
            }
        }
        ExprKind::HandlerLit { methods, .. } | ExprKind::ProtocolLit { methods, .. } => {
            for m in methods {
                match &m.body {
                    HandlerMethodBody::Expr(e) => count_pure_in_expr(e, pure_methods, recv_type, counts, first_spans, captured, true),
                    HandlerMethodBody::Block(b) => count_pure_in_block(b, pure_methods, recv_type, counts, first_spans, captured, true),
                }
            }
        }
        ExprKind::Block(b) => count_pure_in_block(b, pure_methods, recv_type, counts, first_spans, captured, in_closure),
        ExprKind::If { cond, then, else_ } => {
            count_pure_in_expr(cond, pure_methods, recv_type, counts, first_spans, captured, in_closure);
            count_pure_in_block(then, pure_methods, recv_type, counts, first_spans, captured, in_closure);
            if let Some(eb) = else_ {
                match eb {
                    ElseBranch::Block(b) => count_pure_in_block(b, pure_methods, recv_type, counts, first_spans, captured, in_closure),
                    ElseBranch::If(e) => count_pure_in_expr(e, pure_methods, recv_type, counts, first_spans, captured, in_closure),
                }
            }
        }
        ExprKind::IfLet { scrutinee, then, else_, .. } => {
            count_pure_in_expr(scrutinee, pure_methods, recv_type, counts, first_spans, captured, in_closure);
            count_pure_in_block(then, pure_methods, recv_type, counts, first_spans, captured, in_closure);
            if let Some(eb) = else_ {
                match eb {
                    ElseBranch::Block(b) => count_pure_in_block(b, pure_methods, recv_type, counts, first_spans, captured, in_closure),
                    ElseBranch::If(e) => count_pure_in_expr(e, pure_methods, recv_type, counts, first_spans, captured, in_closure),
                }
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            count_pure_in_expr(scrutinee, pure_methods, recv_type, counts, first_spans, captured, in_closure);
            for arm in arms {
                if let Some(g) = &arm.guard {
                    count_pure_in_expr(g, pure_methods, recv_type, counts, first_spans, captured, in_closure);
                }
                match &arm.body {
                    MatchArmBody::Expr(e) => count_pure_in_expr(e, pure_methods, recv_type, counts, first_spans, captured, in_closure),
                    MatchArmBody::Block(b) => count_pure_in_block(b, pure_methods, recv_type, counts, first_spans, captured, in_closure),
                }
            }
        }
        ExprKind::For { iter, body, .. } | ExprKind::ParallelFor { iter, body, .. } => {
            count_pure_in_expr(iter, pure_methods, recv_type, counts, first_spans, captured, in_closure);
            count_pure_in_block(body, pure_methods, recv_type, counts, first_spans, captured, in_closure);
        }
        ExprKind::While { cond, body, .. } => {
            count_pure_in_expr(cond, pure_methods, recv_type, counts, first_spans, captured, in_closure);
            count_pure_in_block(body, pure_methods, recv_type, counts, first_spans, captured, in_closure);
        }
        ExprKind::WhileLet { scrutinee, body, .. } => {
            count_pure_in_expr(scrutinee, pure_methods, recv_type, counts, first_spans, captured, in_closure);
            count_pure_in_block(body, pure_methods, recv_type, counts, first_spans, captured, in_closure);
        }
        ExprKind::Loop { body, .. } => count_pure_in_block(body, pure_methods, recv_type, counts, first_spans, captured, in_closure),
        ExprKind::With { bindings, body } => {
            for wb in bindings {
                count_pure_in_expr(&wb.handler, pure_methods, recv_type, counts, first_spans, captured, in_closure);
            }
            count_pure_in_block(body, pure_methods, recv_type, counts, first_spans, captured, in_closure);
        }
        ExprKind::Forbid { body, .. } | ExprKind::Realtime { body, .. }
        | ExprKind::Detach(body) | ExprKind::Blocking(body) => {
            count_pure_in_block(body, pure_methods, recv_type, counts, first_spans, captured, in_closure);
        }
        ExprKind::Supervised { body, cancel } => {
            count_pure_in_block(body, pure_methods, recv_type, counts, first_spans, captured, in_closure);
            if let Some(c) = cancel { count_pure_in_expr(c, pure_methods, recv_type, counts, first_spans, captured, in_closure); }
        }
        ExprKind::Spawn(e) | ExprKind::Throw(e) | ExprKind::Try(e) | ExprKind::Bang(e)
        | ExprKind::Member { obj: e, .. } | ExprKind::TurboFish { base: e, .. }
        | ExprKind::As(e, _) | ExprKind::Is(e, _) | ExprKind::Unary { operand: e, .. } => {
            count_pure_in_expr(e, pure_methods, recv_type, counts, first_spans, captured, in_closure);
        }
        ExprKind::Coalesce(a, b) | ExprKind::Binary { left: a, right: b, .. } => {
            count_pure_in_expr(a, pure_methods, recv_type, counts, first_spans, captured, in_closure);
            count_pure_in_expr(b, pure_methods, recv_type, counts, first_spans, captured, in_closure);
        }
        ExprKind::Index { obj, index } => {
            count_pure_in_expr(obj, pure_methods, recv_type, counts, first_spans, captured, in_closure);
            count_pure_in_expr(index, pure_methods, recv_type, counts, first_spans, captured, in_closure);
        }
        ExprKind::Call { func, args, trailing } => {
            count_pure_in_expr(func, pure_methods, recv_type, counts, first_spans, captured, in_closure);
            for a in args {
                count_pure_in_expr(a.expr(), pure_methods, recv_type, counts, first_spans, captured, in_closure);
            }
            if let Some(t) = trailing {
                match t {
                    Trailing::Block(b) => count_pure_in_block(b, pure_methods, recv_type, counts, first_spans, captured, in_closure),
                    Trailing::Fn(sb) => match &sb.body {
                        FnBody::Block(b) => count_pure_in_block(b, pure_methods, recv_type, counts, first_spans, captured, true),
                        FnBody::Expr(e) => count_pure_in_expr(e, pure_methods, recv_type, counts, first_spans, captured, true),
                        FnBody::External => {}
                    },
                    Trailing::LegacyBlockWithParams(tb) =>
                        count_pure_in_block(&tb.body, pure_methods, recv_type, counts, first_spans, captured, true),
                }
            }
        }
        ExprKind::InterpolatedStr { parts } => {
            for p in parts {
                if let InterpStrPart::Expr(e) = p {
                    count_pure_in_expr(e, pure_methods, recv_type, counts, first_spans, captured, in_closure);
                }
            }
        }
        ExprKind::ArrayLit(elems) => {
            for el in elems {
                match el {
                    ArrayElem::Item(e) | ArrayElem::Spread(e) =>
                        count_pure_in_expr(e, pure_methods, recv_type, counts, first_spans, captured, in_closure),
                }
            }
        }
        ExprKind::MapLit { elems, .. } => {
            for el in elems {
                match el {
                    MapElem::Pair(k, v) => {
                        count_pure_in_expr(k, pure_methods, recv_type, counts, first_spans, captured, in_closure);
                        count_pure_in_expr(v, pure_methods, recv_type, counts, first_spans, captured, in_closure);
                    }
                    MapElem::Spread(e) => count_pure_in_expr(e, pure_methods, recv_type, counts, first_spans, captured, in_closure),
                }
            }
        }
        ExprKind::RecordLit { fields: rfields, .. } => {
            for rf in rfields {
                if let Some(v) = &rf.value {
                    count_pure_in_expr(v, pure_methods, recv_type, counts, first_spans, captured, in_closure);
                }
            }
        }
        ExprKind::TupleLit(elems) => {
            for el in elems {
                count_pure_in_expr(el, pure_methods, recv_type, counts, first_spans, captured, in_closure);
            }
        }
        ExprKind::Select { arms } => {
            for arm in arms {
                if let Some(g) = &arm.guard {
                    count_pure_in_expr(g, pure_methods, recv_type, counts, first_spans, captured, in_closure);
                }
                count_pure_in_block(&arm.body, pure_methods, recv_type, counts, first_spans, captured, in_closure);
                match &arm.op {
                    SelectOp::Recv { chan, .. } => count_pure_in_expr(chan, pure_methods, recv_type, counts, first_spans, captured, in_closure),
                    SelectOp::Send { chan, value } => {
                        count_pure_in_expr(chan, pure_methods, recv_type, counts, first_spans, captured, in_closure);
                        count_pure_in_expr(value, pure_methods, recv_type, counts, first_spans, captured, in_closure);
                    }
                    SelectOp::Default => {}
                }
            }
        }
        ExprKind::Range { start, end, .. } => {
            if let Some(s) = start { count_pure_in_expr(s, pure_methods, recv_type, counts, first_spans, captured, in_closure); }
            if let Some(e) = end { count_pure_in_expr(e, pure_methods, recv_type, counts, first_spans, captured, in_closure); }
        }
        ExprKind::Forall { range, body, .. } | ExprKind::Exists { range, body, .. } => {
            count_pure_in_expr(range, pure_methods, recv_type, counts, first_spans, captured, in_closure);
            count_pure_in_expr(body, pure_methods, recv_type, counts, first_spans, captured, in_closure);
        }
        ExprKind::Interrupt(opt) => {
            if let Some(e) = opt { count_pure_in_expr(e, pure_methods, recv_type, counts, first_spans, captured, in_closure); }
        }
        ExprKind::TaggedTemplate { tag, args, .. } => {
            count_pure_in_expr(tag, pure_methods, recv_type, counts, first_spans, captured, in_closure);
            for a in args { count_pure_in_expr(a, pure_methods, recv_type, counts, first_spans, captured, in_closure); }
        }
        // Leaves.
        _ => {}
    }
}

/// Plan 123.3.1 V3.1: canonical key for pure-call cache lookup.
/// V3 (args-less): args_key = "".
/// V3.1 (literal args): args_key = canonical repr like "_0i_42i" for
/// IntLit(0) и IntLit(42).
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub(crate) struct PureCallKey {
    pub method: String,
    pub args_key: String,
}

/// Match `Call { func: Member{SelfAccess, name: M}, args }` where
/// (recv_type, M) ∈ pure_methods AND all args are simple literals
/// (V3 args-less или V3.1 literal-args). Returns canonical key.
fn match_self_pure_call(
    e: &Expr,
    pure_methods: &HashSet<(String, String)>,
    recv_type: &str,
) -> Option<PureCallKey> {
    if let ExprKind::Call { func, args, trailing } = &e.kind {
        if trailing.is_some() {
            return None;
        }
        if let ExprKind::Member { obj, name } = &func.kind {
            if matches!(obj.kind, ExprKind::SelfAccess) {
                if pure_methods.contains(&(recv_type.to_string(), name.clone())) {
                    // V3.1: check ALL args are literals.
                    let mut args_key = String::new();
                    for arg in args {
                        let arg_expr = match arg {
                            CallArg::Item(e) => e,
                            CallArg::Spread(_) | CallArg::Named { .. } => return None, // V3.1 simple.
                        };
                        match canonical_literal_repr(arg_expr) {
                            Some(repr) => {
                                args_key.push('_');
                                args_key.push_str(&repr);
                            }
                            None => return None, // Non-literal arg → not eligible.
                        }
                    }
                    return Some(PureCallKey {
                        method: name.clone(),
                        args_key,
                    });
                }
            }
        }
    }
    None
}

/// V3.1: returns canonical String repr if expr is a simple literal,
/// `None` otherwise.
fn canonical_literal_repr(e: &Expr) -> Option<String> {
    match &e.kind {
        ExprKind::IntLit(n) => Some(format!("{}i", n)),
        ExprKind::FloatLit(f) => Some(format!("{}f", f.to_bits())),
        ExprKind::BoolLit(b) => Some(if *b { "T".into() } else { "F".into() }),
        ExprKind::CharLit(c) => Some(format!("{}c", c)),
        ExprKind::UnitLit => Some("U".into()),
        ExprKind::NullPtrLit => Some("N".into()),
        ExprKind::StrLit(s) => {
            // Hash для strings to keep names short.
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            let mut h = DefaultHasher::new();
            s.hash(&mut h);
            Some(format!("s{:x}", h.finish() & 0xFFFFFF))
        }
        // Unary negation на literal: `-5` parses as Unary{Neg, IntLit(5)}.
        ExprKind::Unary { op: UnOp::Neg, operand } => {
            canonical_literal_repr(operand).map(|r| format!("m{}", r))
        }
        _ => None,
    }
}

fn rewrite_pure_calls_in_block(b: &mut Block, renames: &HashMap<String, String>) {
    for s in &mut b.stmts {
        rewrite_pure_calls_in_stmt(s, renames);
    }
    if let Some(t) = &mut b.trailing {
        rewrite_pure_calls_in_expr(t, renames);
    }
}

fn rewrite_pure_calls_in_stmt(s: &mut Stmt, renames: &HashMap<String, String>) {
    match s {
        Stmt::Let(d) => rewrite_pure_calls_in_expr(&mut d.value, renames),
        Stmt::Const(d) => rewrite_pure_calls_in_expr(&mut d.value, renames),
        Stmt::Expr(e) => rewrite_pure_calls_in_expr(e, renames),
        Stmt::Assign { target, value, .. } => {
            rewrite_pure_calls_in_expr(target, renames);
            rewrite_pure_calls_in_expr(value, renames);
        }
        Stmt::Return { value, .. } => {
            if let Some(v) = value {
                rewrite_pure_calls_in_expr(v, renames);
            }
        }
        Stmt::Throw { value, .. } => rewrite_pure_calls_in_expr(value, renames),
        Stmt::Defer { body, .. } | Stmt::ErrDefer { body, .. }
        | Stmt::OkDefer { body, .. } | Stmt::DeferWithResult { body, .. } => {
            rewrite_pure_calls_in_expr(body, renames);
        }
        Stmt::ConsumeScope { init, body, .. } => {
            rewrite_pure_calls_in_expr(init, renames);
            rewrite_pure_calls_in_block(body, renames);
        }
        Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => {
            rewrite_pure_calls_in_expr(expr, renames);
        }
        Stmt::Break(_) | Stmt::Continue(_)
        | Stmt::Apply { .. } | Stmt::Calc { .. } | Stmt::Reveal { .. } => {}
    }
}

fn rewrite_pure_calls_in_expr(e: &mut Expr, renames: &HashMap<String, String>) {
    // Detect `@<method>()` pattern → replace с Ident(local).
    if let ExprKind::Call { func, args, trailing } = &e.kind {
        if args.is_empty() && trailing.is_none() {
            if let ExprKind::Member { obj, name } = &func.kind {
                if matches!(obj.kind, ExprKind::SelfAccess) {
                    if let Some(local) = renames.get(name) {
                        e.kind = ExprKind::Ident(local.clone());
                        return;
                    }
                }
            }
        }
    }
    // Don't recurse into closure bodies — cache locals not in their scope.
    match &e.kind {
        ExprKind::ClosureLight { .. } | ExprKind::ClosureFull(_)
        | ExprKind::Lambda { .. } | ExprKind::HandlerLit { .. }
        | ExprKind::ProtocolLit { .. } => return,
        _ => {}
    }
    // Recurse children.
    match &mut e.kind {
        ExprKind::Block(b) => rewrite_pure_calls_in_block(b, renames),
        ExprKind::If { cond, then, else_ } => {
            rewrite_pure_calls_in_expr(cond, renames);
            rewrite_pure_calls_in_block(then, renames);
            if let Some(eb) = else_ {
                match eb {
                    ElseBranch::Block(b) => rewrite_pure_calls_in_block(b, renames),
                    ElseBranch::If(e) => rewrite_pure_calls_in_expr(e, renames),
                }
            }
        }
        ExprKind::IfLet { scrutinee, then, else_, .. } => {
            rewrite_pure_calls_in_expr(scrutinee, renames);
            rewrite_pure_calls_in_block(then, renames);
            if let Some(eb) = else_ {
                match eb {
                    ElseBranch::Block(b) => rewrite_pure_calls_in_block(b, renames),
                    ElseBranch::If(e) => rewrite_pure_calls_in_expr(e, renames),
                }
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            rewrite_pure_calls_in_expr(scrutinee, renames);
            for arm in arms {
                if let Some(g) = &mut arm.guard { rewrite_pure_calls_in_expr(g, renames); }
                match &mut arm.body {
                    MatchArmBody::Expr(e) => rewrite_pure_calls_in_expr(e, renames),
                    MatchArmBody::Block(b) => rewrite_pure_calls_in_block(b, renames),
                }
            }
        }
        ExprKind::For { iter, body, .. } | ExprKind::ParallelFor { iter, body, .. } => {
            rewrite_pure_calls_in_expr(iter, renames);
            rewrite_pure_calls_in_block(body, renames);
        }
        ExprKind::While { cond, body, .. } => {
            rewrite_pure_calls_in_expr(cond, renames);
            rewrite_pure_calls_in_block(body, renames);
        }
        ExprKind::WhileLet { scrutinee, body, .. } => {
            rewrite_pure_calls_in_expr(scrutinee, renames);
            rewrite_pure_calls_in_block(body, renames);
        }
        ExprKind::Loop { body, .. } => rewrite_pure_calls_in_block(body, renames),
        ExprKind::With { bindings, body } => {
            for wb in bindings { rewrite_pure_calls_in_expr(&mut wb.handler, renames); }
            rewrite_pure_calls_in_block(body, renames);
        }
        ExprKind::Forbid { body, .. } | ExprKind::Realtime { body, .. }
        | ExprKind::Detach(body) | ExprKind::Blocking(body) => {
            rewrite_pure_calls_in_block(body, renames);
        }
        ExprKind::Supervised { body, cancel } => {
            rewrite_pure_calls_in_block(body, renames);
            if let Some(c) = cancel { rewrite_pure_calls_in_expr(c, renames); }
        }
        ExprKind::Spawn(e) | ExprKind::Throw(e) | ExprKind::Try(e) | ExprKind::Bang(e)
        | ExprKind::Member { obj: e, .. } | ExprKind::TurboFish { base: e, .. }
        | ExprKind::As(e, _) | ExprKind::Is(e, _) | ExprKind::Unary { operand: e, .. } => {
            rewrite_pure_calls_in_expr(e, renames);
        }
        ExprKind::Coalesce(a, b) | ExprKind::Binary { left: a, right: b, .. } => {
            rewrite_pure_calls_in_expr(a, renames);
            rewrite_pure_calls_in_expr(b, renames);
        }
        ExprKind::Index { obj, index } => {
            rewrite_pure_calls_in_expr(obj, renames);
            rewrite_pure_calls_in_expr(index, renames);
        }
        ExprKind::Call { func, args, trailing } => {
            rewrite_pure_calls_in_expr(func, renames);
            for a in args {
                match a {
                    CallArg::Item(e) | CallArg::Spread(e) => rewrite_pure_calls_in_expr(e, renames),
                    CallArg::Named { value, .. } => rewrite_pure_calls_in_expr(value, renames),
                }
            }
            if let Some(t) = trailing {
                match t {
                    Trailing::Block(b) => rewrite_pure_calls_in_block(b, renames),
                    Trailing::Fn(_) | Trailing::LegacyBlockWithParams(_) => {}
                }
            }
        }
        ExprKind::InterpolatedStr { parts } => {
            for p in parts {
                if let InterpStrPart::Expr(e) = p { rewrite_pure_calls_in_expr(e, renames); }
            }
        }
        ExprKind::ArrayLit(elems) => {
            for el in elems {
                match el {
                    ArrayElem::Item(e) | ArrayElem::Spread(e) => rewrite_pure_calls_in_expr(e, renames),
                }
            }
        }
        ExprKind::MapLit { elems, .. } => {
            for el in elems {
                match el {
                    MapElem::Pair(k, v) => {
                        rewrite_pure_calls_in_expr(k, renames);
                        rewrite_pure_calls_in_expr(v, renames);
                    }
                    MapElem::Spread(e) => rewrite_pure_calls_in_expr(e, renames),
                }
            }
        }
        ExprKind::RecordLit { fields: rfields, .. } => {
            for rf in rfields {
                if let Some(v) = &mut rf.value { rewrite_pure_calls_in_expr(v, renames); }
            }
        }
        ExprKind::TupleLit(elems) => {
            for el in elems { rewrite_pure_calls_in_expr(el, renames); }
        }
        ExprKind::Select { arms } => {
            for arm in arms {
                if let Some(g) = &mut arm.guard { rewrite_pure_calls_in_expr(g, renames); }
                rewrite_pure_calls_in_block(&mut arm.body, renames);
                match &mut arm.op {
                    SelectOp::Recv { chan, .. } => rewrite_pure_calls_in_expr(chan, renames),
                    SelectOp::Send { chan, value } => {
                        rewrite_pure_calls_in_expr(chan, renames);
                        rewrite_pure_calls_in_expr(value, renames);
                    }
                    SelectOp::Default => {}
                }
            }
        }
        ExprKind::Range { start, end, .. } => {
            if let Some(s) = start { rewrite_pure_calls_in_expr(s, renames); }
            if let Some(e) = end { rewrite_pure_calls_in_expr(e, renames); }
        }
        ExprKind::Forall { range, body, .. } | ExprKind::Exists { range, body, .. } => {
            rewrite_pure_calls_in_expr(range, renames);
            rewrite_pure_calls_in_expr(body, renames);
        }
        ExprKind::Interrupt(opt) => {
            if let Some(e) = opt { rewrite_pure_calls_in_expr(e, renames); }
        }
        ExprKind::TaggedTemplate { tag, args, .. } => {
            rewrite_pure_calls_in_expr(tag, renames);
            for a in args { rewrite_pure_calls_in_expr(a, renames); }
        }
        _ => {}
    }
}

// ===== Plan 123.4 (D217 amend): Chain caching `@a.b.c` =====
//
// V4 phase. Caches nested chain access patterns `@a.b.c` when
// accessed ≥ threshold times. Chain length 2-4 (cfg.chain_max_depth).
//
// Composition: cache_module runs D218 LICM → V4 chain → D219 pure
// → D217 per-fn. V4 emits `ro _at_<a>_<b>_<c>_chain = @<a>.<b>.<c>`
// at body prefix.
//
// Eligibility:
// - Chain length 2..=cfg.chain_max_depth.
// - Occurrence count ≥ cfg.chain_threshold.
// - No top-level @F write для chain root field anywhere in body
//   (V4 conservative; V4.1 refines via per-segment tracking).
// - No closure capture of any chain occurrence.
// - No concurrent body (Spawn/Supervised/etc).
// - Receiver type не protocol/effect/opaque/sum/newtype/alias.

fn chain_cache_fn(f: &mut FnDecl, reg: &FieldRegistry, cfg: &FieldCacheConfig) {
    chain_cache_fn_impl(f, reg, cfg, None)
}

/// Plan 123 V7.2 (2026-06-02): explicit IPA threading for chain-cache.
/// `ipa` carries write_sets для per-root invalidation, replacing CHAIN_IPA_CTX.
fn chain_cache_fn_impl(
    f: &mut FnDecl,
    reg: &FieldRegistry,
    cfg: &FieldCacheConfig,
    ipa: Option<IpaCtx<'_>>,
) {
    let Some(recv) = &f.receiver else { return };
    if recv.kind == ReceiverKind::Static {
        return;
    }
    let type_name = &recv.type_name;
    if reg.skip_types.contains(type_name) {
        return;
    }
    // Skip if receiver type unknown.
    if reg.by_type.get(type_name).is_none() {
        return;
    }
    if f.is_external {
        return;
    }
    if body_has_concurrent(&f.body) {
        return;
    }
    // Plan 123 V7.2 (2026-06-02): explicit IPA ctx replaces CHAIN_IPA_CTX
    // thread-local snapshot. Some == V4.1 per-root invalidation;
    // None == V4 conservative "any write skips chain caching".
    let chain_ipa: Option<(String, HashMap<(String, String), HashSet<String>>)> =
        ipa.map(|i| (i.recv_type.to_string(), i.write_sets.clone()));

    // Collect body's write-set (top-level fields written).
    let body_writes: HashSet<String> = collect_body_writes(&f.body);

    // V4 conservative fallback when IPA disabled: any write → skip.
    let use_ipa_per_root = chain_ipa.is_some();
    if !use_ipa_per_root && !body_writes.is_empty() {
        return;
    }

    // Collect chains with counts.
    let mut counts: HashMap<Vec<String>, usize> = HashMap::new();
    let mut first_spans: HashMap<Vec<String>, crate::diag::Span> = HashMap::new();
    let mut closure_captured: HashSet<Vec<String>> = HashSet::new();
    let max_depth = cfg.chain_max_depth.max(2);
    count_chains_in_body(&f.body, &mut counts, &mut first_spans, &mut closure_captured, max_depth, false);

    let mut local_names: HashSet<String> = HashSet::new();
    for p in &f.params {
        local_names.insert(p.name.clone());
    }
    collect_local_names_fn(f, &mut local_names);

    // Decide eligible chains: count ≥ threshold AND not captured.
    let mut keys: Vec<&Vec<String>> = counts.keys().collect();
    keys.sort();
    let mut to_cache: Vec<(Vec<String>, crate::diag::Span)> = Vec::new();
    let body_span = match &f.body {
        FnBody::Block(b) => b.span,
        FnBody::Expr(e) => e.span,
        FnBody::External => return,
    };
    for path in keys {
        if path.len() < 2 {
            continue;
        }
        if counts[path] < cfg.chain_threshold {
            continue;
        }
        if closure_captured.contains(path) {
            continue;
        }
        // V4.1 per-root invalidation: chain `@a.b.c` invalidated only
        // if root field `a` is in body's write-set OR
        // any intermediate path segment written
        // (V4.1 simpler: just check root + intermediates anyway via
        // path components intersection с body_writes).
        if use_ipa_per_root {
            if path.iter().any(|seg| body_writes.contains(seg)) {
                continue;
            }
        }
        let span = first_spans.get(path).copied().unwrap_or(body_span);
        to_cache.push((path.clone(), span));
        if to_cache.len() >= cfg.max_per_fn {
            break;
        }
    }

    if to_cache.is_empty() {
        return;
    }

    // Generate collision-safe names.
    let mut name_map: HashMap<Vec<String>, String> = HashMap::new();
    for (path, _) in &to_cache {
        let base = format!("_at_{}_chain", path.join("_"));
        let mut chosen = base.clone();
        let mut suffix = 0usize;
        while local_names.contains(&chosen) {
            suffix += 1;
            chosen = format!("{}_{}", base, suffix);
        }
        local_names.insert(chosen.clone());
        name_map.insert(path.clone(), chosen);
    }

    // Coerce Expr body → Block для prepend.
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

    // Rewrite chain access sites с cache idents.
    if let FnBody::Block(b) = &mut f.body {
        rewrite_chains_in_block(b, &name_map);
    }

    // Prepend cache let statements.
    if let FnBody::Block(b) = &mut f.body {
        let mut prefix: Vec<Stmt> = Vec::with_capacity(to_cache.len());
        for (path, span) in &to_cache {
            let local_name = &name_map[path];
            let access = build_chain_expr(path, *span);
            prefix.push(Stmt::Let(LetDecl {
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
            }));
        }
        prefix.append(&mut b.stmts);
        b.stmts = prefix;
    }
}

/// Build chain expression `@a.b.c` from path components.
fn build_chain_expr(path: &[String], span: crate::diag::Span) -> Expr {
    let mut current = Expr {
        kind: ExprKind::SelfAccess,
        span,
    };
    for name in path {
        current = Expr {
            kind: ExprKind::Member {
                obj: Box::new(current),
                name: name.clone(),
            },
            span,
        };
    }
    current
}

/// Extract canonical path components от Member chain rooted at
/// SelfAccess. Returns `Some(path)` для `@a.b.c` (path = ["a","b","c"]),
/// `None` otherwise.
fn extract_chain_path(e: &Expr) -> Option<Vec<String>> {
    let mut path: Vec<String> = Vec::new();
    let mut cur = e;
    loop {
        match &cur.kind {
            ExprKind::Member { obj, name } => {
                path.push(name.clone());
                cur = obj;
            }
            ExprKind::SelfAccess => {
                path.reverse();
                return Some(path);
            }
            _ => return None,
        }
    }
}

fn count_chains_in_body(
    body: &FnBody,
    counts: &mut HashMap<Vec<String>, usize>,
    first_spans: &mut HashMap<Vec<String>, crate::diag::Span>,
    captured: &mut HashSet<Vec<String>>,
    max_depth: usize,
    in_closure: bool,
) {
    match body {
        FnBody::Block(b) => count_chains_in_block(b, counts, first_spans, captured, max_depth, in_closure),
        FnBody::Expr(e) => count_chains_in_expr(e, counts, first_spans, captured, max_depth, in_closure),
        FnBody::External => {}
    }
}

fn count_chains_in_block(
    b: &Block,
    counts: &mut HashMap<Vec<String>, usize>,
    first_spans: &mut HashMap<Vec<String>, crate::diag::Span>,
    captured: &mut HashSet<Vec<String>>,
    max_depth: usize,
    in_closure: bool,
) {
    for s in &b.stmts {
        count_chains_in_stmt(s, counts, first_spans, captured, max_depth, in_closure);
    }
    if let Some(t) = &b.trailing {
        count_chains_in_expr(t, counts, first_spans, captured, max_depth, in_closure);
    }
}

fn count_chains_in_stmt(
    s: &Stmt,
    counts: &mut HashMap<Vec<String>, usize>,
    first_spans: &mut HashMap<Vec<String>, crate::diag::Span>,
    captured: &mut HashSet<Vec<String>>,
    max_depth: usize,
    in_closure: bool,
) {
    match s {
        Stmt::Let(d) => count_chains_in_expr(&d.value, counts, first_spans, captured, max_depth, in_closure),
        Stmt::Const(d) => count_chains_in_expr(&d.value, counts, first_spans, captured, max_depth, in_closure),
        Stmt::Expr(e) => count_chains_in_expr(e, counts, first_spans, captured, max_depth, in_closure),
        Stmt::Assign { target, value, .. } => {
            count_chains_in_expr(target, counts, first_spans, captured, max_depth, in_closure);
            count_chains_in_expr(value, counts, first_spans, captured, max_depth, in_closure);
        }
        Stmt::Return { value, .. } => {
            if let Some(v) = value {
                count_chains_in_expr(v, counts, first_spans, captured, max_depth, in_closure);
            }
        }
        Stmt::Throw { value, .. } => count_chains_in_expr(value, counts, first_spans, captured, max_depth, in_closure),
        Stmt::Defer { body, .. } | Stmt::ErrDefer { body, .. }
        | Stmt::OkDefer { body, .. } | Stmt::DeferWithResult { body, .. } => {
            count_chains_in_expr(body, counts, first_spans, captured, max_depth, in_closure);
        }
        Stmt::ConsumeScope { init, body, .. } => {
            count_chains_in_expr(init, counts, first_spans, captured, max_depth, in_closure);
            count_chains_in_block(body, counts, first_spans, captured, max_depth, in_closure);
        }
        Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => {
            count_chains_in_expr(expr, counts, first_spans, captured, max_depth, in_closure);
        }
        Stmt::Break(_) | Stmt::Continue(_)
        | Stmt::Apply { .. } | Stmt::Calc { .. } | Stmt::Reveal { .. } => {}
    }
}

fn count_chains_in_expr(
    e: &Expr,
    counts: &mut HashMap<Vec<String>, usize>,
    first_spans: &mut HashMap<Vec<String>, crate::diag::Span>,
    captured: &mut HashSet<Vec<String>>,
    max_depth: usize,
    in_closure: bool,
) {
    // Check if this expr is a chain rooted at SelfAccess.
    if let Some(path) = extract_chain_path(e) {
        if path.len() >= 2 && path.len() <= max_depth {
            if in_closure {
                captured.insert(path.clone());
            } else {
                *counts.entry(path.clone()).or_insert(0) += 1;
                first_spans.entry(path).or_insert(e.span);
            }
            // Don't recurse into this chain — fully consumed.
            return;
        }
    }
    // Recurse children, switching in_closure flag on closure entry.
    match &e.kind {
        ExprKind::Lambda { body, .. } => count_chains_in_expr(body, counts, first_spans, captured, max_depth, true),
        ExprKind::ClosureLight { body, .. } => {
            match body {
                ClosureBody::Expr(e) => count_chains_in_expr(e, counts, first_spans, captured, max_depth, true),
                ClosureBody::Block(b) => count_chains_in_block(b, counts, first_spans, captured, max_depth, true),
            }
        }
        ExprKind::ClosureFull(sb) => match &sb.body {
            FnBody::Expr(e) => count_chains_in_expr(e, counts, first_spans, captured, max_depth, true),
            FnBody::Block(b) => count_chains_in_block(b, counts, first_spans, captured, max_depth, true),
            FnBody::External => {}
        },
        ExprKind::HandlerLit { methods, .. } | ExprKind::ProtocolLit { methods, .. } => {
            for m in methods {
                match &m.body {
                    HandlerMethodBody::Expr(e) => count_chains_in_expr(e, counts, first_spans, captured, max_depth, true),
                    HandlerMethodBody::Block(b) => count_chains_in_block(b, counts, first_spans, captured, max_depth, true),
                }
            }
        }
        ExprKind::Block(b) => count_chains_in_block(b, counts, first_spans, captured, max_depth, in_closure),
        ExprKind::If { cond, then, else_ } => {
            count_chains_in_expr(cond, counts, first_spans, captured, max_depth, in_closure);
            count_chains_in_block(then, counts, first_spans, captured, max_depth, in_closure);
            if let Some(eb) = else_ {
                match eb {
                    ElseBranch::Block(b) => count_chains_in_block(b, counts, first_spans, captured, max_depth, in_closure),
                    ElseBranch::If(e) => count_chains_in_expr(e, counts, first_spans, captured, max_depth, in_closure),
                }
            }
        }
        ExprKind::IfLet { scrutinee, then, else_, .. } => {
            count_chains_in_expr(scrutinee, counts, first_spans, captured, max_depth, in_closure);
            count_chains_in_block(then, counts, first_spans, captured, max_depth, in_closure);
            if let Some(eb) = else_ {
                match eb {
                    ElseBranch::Block(b) => count_chains_in_block(b, counts, first_spans, captured, max_depth, in_closure),
                    ElseBranch::If(e) => count_chains_in_expr(e, counts, first_spans, captured, max_depth, in_closure),
                }
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            count_chains_in_expr(scrutinee, counts, first_spans, captured, max_depth, in_closure);
            for arm in arms {
                if let Some(g) = &arm.guard {
                    count_chains_in_expr(g, counts, first_spans, captured, max_depth, in_closure);
                }
                match &arm.body {
                    MatchArmBody::Expr(e) => count_chains_in_expr(e, counts, first_spans, captured, max_depth, in_closure),
                    MatchArmBody::Block(b) => count_chains_in_block(b, counts, first_spans, captured, max_depth, in_closure),
                }
            }
        }
        ExprKind::For { iter, body, .. } | ExprKind::ParallelFor { iter, body, .. } => {
            count_chains_in_expr(iter, counts, first_spans, captured, max_depth, in_closure);
            count_chains_in_block(body, counts, first_spans, captured, max_depth, in_closure);
        }
        ExprKind::While { cond, body, .. } => {
            count_chains_in_expr(cond, counts, first_spans, captured, max_depth, in_closure);
            count_chains_in_block(body, counts, first_spans, captured, max_depth, in_closure);
        }
        ExprKind::WhileLet { scrutinee, body, .. } => {
            count_chains_in_expr(scrutinee, counts, first_spans, captured, max_depth, in_closure);
            count_chains_in_block(body, counts, first_spans, captured, max_depth, in_closure);
        }
        ExprKind::Loop { body, .. } => count_chains_in_block(body, counts, first_spans, captured, max_depth, in_closure),
        ExprKind::With { bindings, body } => {
            for wb in bindings {
                count_chains_in_expr(&wb.handler, counts, first_spans, captured, max_depth, in_closure);
            }
            count_chains_in_block(body, counts, first_spans, captured, max_depth, in_closure);
        }
        ExprKind::Forbid { body, .. } | ExprKind::Realtime { body, .. }
        | ExprKind::Detach(body) | ExprKind::Blocking(body) => {
            count_chains_in_block(body, counts, first_spans, captured, max_depth, in_closure);
        }
        ExprKind::Supervised { body, cancel } => {
            count_chains_in_block(body, counts, first_spans, captured, max_depth, in_closure);
            if let Some(c) = cancel {
                count_chains_in_expr(c, counts, first_spans, captured, max_depth, in_closure);
            }
        }
        ExprKind::Spawn(e) | ExprKind::Throw(e) | ExprKind::Try(e) | ExprKind::Bang(e)
        | ExprKind::Member { obj: e, .. } | ExprKind::TurboFish { base: e, .. }
        | ExprKind::As(e, _) | ExprKind::Is(e, _) | ExprKind::Unary { operand: e, .. } => {
            count_chains_in_expr(e, counts, first_spans, captured, max_depth, in_closure);
        }
        ExprKind::Coalesce(a, b) | ExprKind::Binary { left: a, right: b, .. } => {
            count_chains_in_expr(a, counts, first_spans, captured, max_depth, in_closure);
            count_chains_in_expr(b, counts, first_spans, captured, max_depth, in_closure);
        }
        ExprKind::Index { obj, index } => {
            count_chains_in_expr(obj, counts, first_spans, captured, max_depth, in_closure);
            count_chains_in_expr(index, counts, first_spans, captured, max_depth, in_closure);
        }
        ExprKind::Call { func, args, trailing } => {
            // V4 — chain detection should NOT include method-dispatch
            // names. For `@a.b.method()`, recurse only into the
            // receiver `@a.b` (obj of func Member), не into func
            // itself (which would count `@a.b.method` as 3-chain).
            match &func.kind {
                ExprKind::Member { obj, .. } => {
                    count_chains_in_expr(obj, counts, first_spans, captured, max_depth, in_closure);
                }
                _ => {
                    count_chains_in_expr(func, counts, first_spans, captured, max_depth, in_closure);
                }
            }
            for a in args {
                count_chains_in_expr(a.expr(), counts, first_spans, captured, max_depth, in_closure);
            }
            if let Some(t) = trailing {
                match t {
                    Trailing::Block(b) => count_chains_in_block(b, counts, first_spans, captured, max_depth, in_closure),
                    Trailing::Fn(sb) => match &sb.body {
                        FnBody::Block(b) => count_chains_in_block(b, counts, first_spans, captured, max_depth, true),
                        FnBody::Expr(e) => count_chains_in_expr(e, counts, first_spans, captured, max_depth, true),
                        FnBody::External => {}
                    },
                    Trailing::LegacyBlockWithParams(tb) => count_chains_in_block(&tb.body, counts, first_spans, captured, max_depth, true),
                }
            }
        }
        ExprKind::InterpolatedStr { parts } => {
            for p in parts {
                if let InterpStrPart::Expr(e) = p {
                    count_chains_in_expr(e, counts, first_spans, captured, max_depth, in_closure);
                }
            }
        }
        ExprKind::ArrayLit(elems) => {
            for el in elems {
                match el {
                    ArrayElem::Item(e) | ArrayElem::Spread(e) =>
                        count_chains_in_expr(e, counts, first_spans, captured, max_depth, in_closure),
                }
            }
        }
        ExprKind::MapLit { elems, .. } => {
            for el in elems {
                match el {
                    MapElem::Pair(k, v) => {
                        count_chains_in_expr(k, counts, first_spans, captured, max_depth, in_closure);
                        count_chains_in_expr(v, counts, first_spans, captured, max_depth, in_closure);
                    }
                    MapElem::Spread(e) => count_chains_in_expr(e, counts, first_spans, captured, max_depth, in_closure),
                }
            }
        }
        ExprKind::RecordLit { fields: rfields, .. } => {
            for rf in rfields {
                if let Some(v) = &rf.value {
                    count_chains_in_expr(v, counts, first_spans, captured, max_depth, in_closure);
                }
            }
        }
        ExprKind::TupleLit(elems) => {
            for el in elems {
                count_chains_in_expr(el, counts, first_spans, captured, max_depth, in_closure);
            }
        }
        ExprKind::Select { arms } => {
            for arm in arms {
                if let Some(g) = &arm.guard {
                    count_chains_in_expr(g, counts, first_spans, captured, max_depth, in_closure);
                }
                count_chains_in_block(&arm.body, counts, first_spans, captured, max_depth, in_closure);
                match &arm.op {
                    SelectOp::Recv { chan, .. } => count_chains_in_expr(chan, counts, first_spans, captured, max_depth, in_closure),
                    SelectOp::Send { chan, value } => {
                        count_chains_in_expr(chan, counts, first_spans, captured, max_depth, in_closure);
                        count_chains_in_expr(value, counts, first_spans, captured, max_depth, in_closure);
                    }
                    SelectOp::Default => {}
                }
            }
        }
        ExprKind::Range { start, end, .. } => {
            if let Some(s) = start { count_chains_in_expr(s, counts, first_spans, captured, max_depth, in_closure); }
            if let Some(e) = end { count_chains_in_expr(e, counts, first_spans, captured, max_depth, in_closure); }
        }
        ExprKind::Forall { range, body, .. } | ExprKind::Exists { range, body, .. } => {
            count_chains_in_expr(range, counts, first_spans, captured, max_depth, in_closure);
            count_chains_in_expr(body, counts, first_spans, captured, max_depth, in_closure);
        }
        ExprKind::Interrupt(opt) => {
            if let Some(e) = opt { count_chains_in_expr(e, counts, first_spans, captured, max_depth, in_closure); }
        }
        ExprKind::TaggedTemplate { tag, args, .. } => {
            count_chains_in_expr(tag, counts, first_spans, captured, max_depth, in_closure);
            for a in args { count_chains_in_expr(a, counts, first_spans, captured, max_depth, in_closure); }
        }
        _ => {}
    }
}

fn rewrite_chains_in_block(b: &mut Block, name_map: &HashMap<Vec<String>, String>) {
    for s in &mut b.stmts {
        rewrite_chains_in_stmt(s, name_map);
    }
    if let Some(t) = &mut b.trailing {
        rewrite_chains_in_expr(t, name_map);
    }
}

fn rewrite_chains_in_stmt(s: &mut Stmt, name_map: &HashMap<Vec<String>, String>) {
    match s {
        Stmt::Let(d) => rewrite_chains_in_expr(&mut d.value, name_map),
        Stmt::Const(d) => rewrite_chains_in_expr(&mut d.value, name_map),
        Stmt::Expr(e) => rewrite_chains_in_expr(e, name_map),
        Stmt::Assign { target, value, .. } => {
            rewrite_chains_in_expr(target, name_map);
            rewrite_chains_in_expr(value, name_map);
        }
        Stmt::Return { value, .. } => {
            if let Some(v) = value { rewrite_chains_in_expr(v, name_map); }
        }
        Stmt::Throw { value, .. } => rewrite_chains_in_expr(value, name_map),
        Stmt::Defer { body, .. } | Stmt::ErrDefer { body, .. }
        | Stmt::OkDefer { body, .. } | Stmt::DeferWithResult { body, .. } => {
            rewrite_chains_in_expr(body, name_map);
        }
        Stmt::ConsumeScope { init, body, .. } => {
            rewrite_chains_in_expr(init, name_map);
            rewrite_chains_in_block(body, name_map);
        }
        Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => {
            rewrite_chains_in_expr(expr, name_map);
        }
        Stmt::Break(_) | Stmt::Continue(_)
        | Stmt::Apply { .. } | Stmt::Calc { .. } | Stmt::Reveal { .. } => {}
    }
}

fn rewrite_chains_in_expr(e: &mut Expr, name_map: &HashMap<Vec<String>, String>) {
    // Check if this expr is a known chain.
    if let Some(path) = extract_chain_path(e) {
        if let Some(local) = name_map.get(&path) {
            e.kind = ExprKind::Ident(local.clone());
            return;
        }
    }
    // Don't recurse into closure bodies.
    match &e.kind {
        ExprKind::ClosureLight { .. } | ExprKind::ClosureFull(_)
        | ExprKind::Lambda { .. } | ExprKind::HandlerLit { .. }
        | ExprKind::ProtocolLit { .. } => return,
        _ => {}
    }
    // Recurse children.
    match &mut e.kind {
        ExprKind::Block(b) => rewrite_chains_in_block(b, name_map),
        ExprKind::If { cond, then, else_ } => {
            rewrite_chains_in_expr(cond, name_map);
            rewrite_chains_in_block(then, name_map);
            if let Some(eb) = else_ {
                match eb {
                    ElseBranch::Block(b) => rewrite_chains_in_block(b, name_map),
                    ElseBranch::If(e) => rewrite_chains_in_expr(e, name_map),
                }
            }
        }
        ExprKind::IfLet { scrutinee, then, else_, .. } => {
            rewrite_chains_in_expr(scrutinee, name_map);
            rewrite_chains_in_block(then, name_map);
            if let Some(eb) = else_ {
                match eb {
                    ElseBranch::Block(b) => rewrite_chains_in_block(b, name_map),
                    ElseBranch::If(e) => rewrite_chains_in_expr(e, name_map),
                }
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            rewrite_chains_in_expr(scrutinee, name_map);
            for arm in arms {
                if let Some(g) = &mut arm.guard { rewrite_chains_in_expr(g, name_map); }
                match &mut arm.body {
                    MatchArmBody::Expr(e) => rewrite_chains_in_expr(e, name_map),
                    MatchArmBody::Block(b) => rewrite_chains_in_block(b, name_map),
                }
            }
        }
        ExprKind::For { iter, body, .. } | ExprKind::ParallelFor { iter, body, .. } => {
            rewrite_chains_in_expr(iter, name_map);
            rewrite_chains_in_block(body, name_map);
        }
        ExprKind::While { cond, body, .. } => {
            rewrite_chains_in_expr(cond, name_map);
            rewrite_chains_in_block(body, name_map);
        }
        ExprKind::WhileLet { scrutinee, body, .. } => {
            rewrite_chains_in_expr(scrutinee, name_map);
            rewrite_chains_in_block(body, name_map);
        }
        ExprKind::Loop { body, .. } => rewrite_chains_in_block(body, name_map),
        ExprKind::With { bindings, body } => {
            for wb in bindings { rewrite_chains_in_expr(&mut wb.handler, name_map); }
            rewrite_chains_in_block(body, name_map);
        }
        ExprKind::Forbid { body, .. } | ExprKind::Realtime { body, .. }
        | ExprKind::Detach(body) | ExprKind::Blocking(body) => {
            rewrite_chains_in_block(body, name_map);
        }
        ExprKind::Supervised { body, cancel } => {
            rewrite_chains_in_block(body, name_map);
            if let Some(c) = cancel { rewrite_chains_in_expr(c, name_map); }
        }
        ExprKind::Spawn(e) | ExprKind::Throw(e) | ExprKind::Try(e) | ExprKind::Bang(e)
        | ExprKind::Member { obj: e, .. } | ExprKind::TurboFish { base: e, .. }
        | ExprKind::As(e, _) | ExprKind::Is(e, _) | ExprKind::Unary { operand: e, .. } => {
            rewrite_chains_in_expr(e, name_map);
        }
        ExprKind::Coalesce(a, b) | ExprKind::Binary { left: a, right: b, .. } => {
            rewrite_chains_in_expr(a, name_map);
            rewrite_chains_in_expr(b, name_map);
        }
        ExprKind::Index { obj, index } => {
            rewrite_chains_in_expr(obj, name_map);
            rewrite_chains_in_expr(index, name_map);
        }
        ExprKind::Call { func, args, trailing } => {
            // For `@a.b.method()` calls — recurse only into the
            // receiver (obj of func Member), не into func itself
            // (method dispatch, not chain).
            if let ExprKind::Member { obj, .. } = &mut func.kind {
                rewrite_chains_in_expr(obj, name_map);
            } else {
                rewrite_chains_in_expr(func, name_map);
            }
            for a in args {
                match a {
                    CallArg::Item(e) | CallArg::Spread(e) => rewrite_chains_in_expr(e, name_map),
                    CallArg::Named { value, .. } => rewrite_chains_in_expr(value, name_map),
                }
            }
            if let Some(t) = trailing {
                match t {
                    Trailing::Block(b) => rewrite_chains_in_block(b, name_map),
                    Trailing::Fn(_) | Trailing::LegacyBlockWithParams(_) => {}
                }
            }
        }
        ExprKind::InterpolatedStr { parts } => {
            for p in parts {
                if let InterpStrPart::Expr(e) = p { rewrite_chains_in_expr(e, name_map); }
            }
        }
        ExprKind::ArrayLit(elems) => {
            for el in elems {
                match el {
                    ArrayElem::Item(e) | ArrayElem::Spread(e) => rewrite_chains_in_expr(e, name_map),
                }
            }
        }
        ExprKind::MapLit { elems, .. } => {
            for el in elems {
                match el {
                    MapElem::Pair(k, v) => {
                        rewrite_chains_in_expr(k, name_map);
                        rewrite_chains_in_expr(v, name_map);
                    }
                    MapElem::Spread(e) => rewrite_chains_in_expr(e, name_map),
                }
            }
        }
        ExprKind::RecordLit { fields: rfields, .. } => {
            for rf in rfields {
                if let Some(v) = &mut rf.value { rewrite_chains_in_expr(v, name_map); }
            }
        }
        ExprKind::TupleLit(elems) => {
            for el in elems { rewrite_chains_in_expr(el, name_map); }
        }
        ExprKind::Select { arms } => {
            for arm in arms {
                if let Some(g) = &mut arm.guard { rewrite_chains_in_expr(g, name_map); }
                rewrite_chains_in_block(&mut arm.body, name_map);
                match &mut arm.op {
                    SelectOp::Recv { chan, .. } => rewrite_chains_in_expr(chan, name_map),
                    SelectOp::Send { chan, value } => {
                        rewrite_chains_in_expr(chan, name_map);
                        rewrite_chains_in_expr(value, name_map);
                    }
                    SelectOp::Default => {}
                }
            }
        }
        ExprKind::Range { start, end, .. } => {
            if let Some(s) = start { rewrite_chains_in_expr(s, name_map); }
            if let Some(e) = end { rewrite_chains_in_expr(e, name_map); }
        }
        ExprKind::Forall { range, body, .. } | ExprKind::Exists { range, body, .. } => {
            rewrite_chains_in_expr(range, name_map);
            rewrite_chains_in_expr(body, name_map);
        }
        ExprKind::Interrupt(opt) => {
            if let Some(e) = opt { rewrite_chains_in_expr(e, name_map); }
        }
        ExprKind::TaggedTemplate { tag, args, .. } => {
            rewrite_chains_in_expr(tag, name_map);
            for a in args { rewrite_chains_in_expr(a, name_map); }
        }
        _ => {}
    }
}

// ===== AST-LEVEL UNIT TESTS (semantic equivalence verification §8.1 method #1) =====
//
// V1 verification methods 2-5 (codegen diff / runtime / property /
// regression) — через nova_tests/plan123_1/*.nv fixtures.
// Тесты в этом mod — направленные на edge cases которые сложно или
// дорого тестировать through runtime (e.g. closure capture detection,
// protocol receiver skip).

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;

    fn run_pass(src: &str, cfg: FieldCacheConfig) -> Module {
        let mut module = parse(src).expect("parse");
        cache_module(&mut module, &cfg);
        module
    }

    fn find_fn<'a>(module: &'a Module, name: &str) -> &'a FnDecl {
        for item in &module.items {
            if let Item::Fn(f) = item {
                if f.name == name {
                    return f;
                }
            }
        }
        panic!("fn {} not found", name);
    }

    fn count_prefix_lets(f: &FnDecl) -> usize {
        if let FnBody::Block(b) = &f.body {
            b.stmts.iter().take_while(|s| {
                matches!(s, Stmt::Let(d)
                    if matches!(&d.pattern, Pattern::Ident { name, .. } if name.starts_with("_at_")))
            }).count()
        } else {
            0
        }
    }

    /// A1.1: ro field accessed 2+ раз → cache emitted.
    #[test]
    fn ro_two_reads_cached() {
        let src = r#"
module testmod.ro_cached
type Point { ro x int, ro y int }
fn Point @sum_squared() -> int { @x * @x + @y * @y }
"#;
        let m = run_pass(src, FieldCacheConfig::default());
        let f = find_fn(&m, "sum_squared");
        assert_eq!(count_prefix_lets(f), 2, "expected 2 ro caches");
    }

    /// A1.9: single read below threshold → no cache.
    #[test]
    fn ro_single_read_not_cached() {
        let src = r#"
module testmod.single
type Point { ro x int }
fn Point @just_x() -> int { @x }
"#;
        let m = run_pass(src, FieldCacheConfig::default());
        let f = find_fn(&m, "just_x");
        assert_eq!(count_prefix_lets(f), 0);
    }

    /// A1.5: escape hatch — threshold=0 disables полностью.
    #[test]
    fn escape_hatch_threshold_zero() {
        let src = r#"
module testmod.escape
type P { ro x int }
fn P @three() -> int { @x + @x + @x }
"#;
        let cfg = FieldCacheConfig::from_threshold(0, 8);
        let m = run_pass(src, cfg);
        let f = find_fn(&m, "three");
        assert_eq!(count_prefix_lets(f), 0);
    }

    /// A1.3: closure capturing @F → caching skipped for F (conservative).
    #[test]
    fn closure_capture_skips_cache() {
        let src = r#"
module testmod.closure
type Box { ro v int }
fn Box @sum() -> int {
    ro f = || @v + 1
    @v + @v + @v
}
"#;
        let m = run_pass(src, FieldCacheConfig::default());
        let f = find_fn(&m, "sum");
        // @v accessed 3 times in main body — would be cached if not
        // for closure. Closure body refs @v → captured → skipped.
        assert_eq!(count_prefix_lets(f), 0,
            "closure captures @v → cache skipped");
    }

    /// A1.4: ANY call invalidates mut cache prefix region.
    #[test]
    fn mut_call_boundary_no_cache_after() {
        let src = r#"
module testmod.mut_call
type C { mut v int }
fn C.foo(x int) -> int { x + 1 }
fn C @work() -> int {
    ro y = @v + @v
    ro _ = C.foo(0)
    @v + @v
}
"#;
        let m = run_pass(src, FieldCacheConfig::default());
        let f = find_fn(&m, "work");
        // Prefix region: first 2 reads of @v before Call boundary →
        // qualify (count=2 ≥ threshold). Mut cache emitted.
        assert_eq!(count_prefix_lets(f), 1,
            "mut field cached for prefix region");
    }

    /// A1.4 corollary: write boundary truncates mut prefix region.
    #[test]
    fn mut_write_boundary_truncates() {
        let src = r#"
module testmod.mut_write
type C { mut v int }
fn C mut @work() -> int {
    ro a = @v + @v
    @v = a
    @v + @v
}
"#;
        let m = run_pass(src, FieldCacheConfig::default());
        let f = find_fn(&m, "work");
        // 2 reads BEFORE write → cached.
        assert_eq!(count_prefix_lets(f), 1);
    }

    /// Protocol receiver — skipped полностью (vtable dispatch).
    #[test]
    fn protocol_receiver_skipped() {
        let src = r#"
module testmod.proto
type Counted protocol { count() -> int }
fn Counted @double_count() -> int { @count() + @count() }
"#;
        // Note: protocol method body using @count() syntactically valid
        // в parser; type-checker может reject это. Здесь мы только
        // проверяем что pass не падает и не emit'ит cache для protocol
        // receiver. parse() может fail если syntax не allowed — в
        // таком случае test skip'нется.
        let parsed = parse(src);
        if let Ok(mut module) = parsed {
            cache_module(&mut module, &FieldCacheConfig::default());
            let f = find_fn(&module, "double_count");
            assert_eq!(count_prefix_lets(f), 0,
                "protocol receiver — no caching");
        }
    }

    /// Effect receiver — skipped.
    #[test]
    fn effect_receiver_skipped() {
        let src = r#"
module testmod.eff
type Log effect { write(s str) -> () }
"#;
        // Just verify no panic — effect types have no record fields,
        // so registry put в skip_types.
        let mut m = parse(src).expect("parse");
        cache_module(&mut m, &FieldCacheConfig::default());
    }

    /// Name collision: user has `_at_x` local → suffix appended.
    #[test]
    fn name_collision_suffix() {
        let src = r#"
module testmod.collision
type Box { ro x int }
fn Box @collide() -> int {
    ro _at_x = 99
    @x + @x + _at_x
}
"#;
        let m = run_pass(src, FieldCacheConfig::default());
        let f = find_fn(&m, "collide");
        // Cache local should be `_at_x_1` (suffix added due to user-
        // local collision).
        if let FnBody::Block(b) = &f.body {
            // Find the first generated cache let (must be _at_x_1
            // because user's _at_x = 99 occupies the bare name).
            let cache_let_name = b.stmts.iter().find_map(|s| {
                if let Stmt::Let(d) = s {
                    if let Pattern::Ident { name, .. } = &d.pattern {
                        // Must come from `@x` access — value should be
                        // Member{SelfAccess, "x"}.
                        if let ExprKind::Member { obj, name: fname } = &d.value.kind {
                            if matches!(obj.kind, ExprKind::SelfAccess) && fname == "x" {
                                return Some(name.clone());
                            }
                        }
                    }
                }
                None
            });
            assert_eq!(cache_let_name.as_deref(), Some("_at_x_1"),
                "expected suffix due to collision");
        } else {
            panic!("expected Block body");
        }
    }

    /// Max-per-fn cap — limited number of caches per fn.
    #[test]
    fn max_per_fn_cap() {
        let src = r#"
module testmod.cap
type Many { ro a int, ro b int, ro c int, ro d int }
fn Many @sum() -> int {
    @a + @a + @b + @b + @c + @c + @d + @d
}
"#;
        let cfg = FieldCacheConfig {
            enabled: true,
            threshold: 2,
            max_per_fn: 2,
            ..FieldCacheConfig::default()
        };
        let m = run_pass(src, cfg);
        let f = find_fn(&m, "sum");
        // Only 2 caches (max_per_fn=2). Fields sorted by name → a,b.
        assert_eq!(count_prefix_lets(f), 2);
    }

    /// Static-method receiver — no @field access → skip.
    #[test]
    fn static_receiver_skipped() {
        let src = r#"
module testmod.static_recv
type P { ro x int }
fn P.constant() -> int { 42 }
"#;
        // Static methods have no @field reads anyway; verify pass
        // doesn't panic + doesn't emit cache.
        let m = run_pass(src, FieldCacheConfig::default());
        let f = find_fn(&m, "constant");
        assert_eq!(count_prefix_lets(f), 0);
    }

    /// Free function (no receiver) — skipped entirely.
    #[test]
    fn free_fn_skipped() {
        let src = r#"
module testmod.free
type P { ro x int }
fn helper(p int) -> int { p + 1 }
"#;
        let m = run_pass(src, FieldCacheConfig::default());
        let f = find_fn(&m, "helper");
        assert_eq!(count_prefix_lets(f), 0);
    }

    /// Sum-type receiver — skipped (no direct record fields).
    #[test]
    fn sum_type_receiver_skipped() {
        let src = r#"
module testmod.sum
type Maybe | None | Some(int)
fn Maybe @is_some() -> bool {
    match @ {
        Some(_) => true,
        None => false,
    }
}
"#;
        // Test mostly for non-panic. Sum types are in skip_types.
        let parsed = parse(src);
        if let Ok(mut module) = parsed {
            cache_module(&mut module, &FieldCacheConfig::default());
        }
    }

    /// Determinism: same input → same output (sorted field iteration).
    #[test]
    fn deterministic_ordering() {
        let src = r#"
module testmod.deterministic
type P { ro b int, ro a int, ro c int }
fn P @sum() -> int { @c + @b + @a + @c + @b + @a }
"#;
        let m1 = run_pass(src, FieldCacheConfig::default());
        let m2 = run_pass(src, FieldCacheConfig::default());
        let f1 = find_fn(&m1, "sum");
        let f2 = find_fn(&m2, "sum");
        // Compare cache local order in prefix.
        let names1: Vec<String> = if let FnBody::Block(b) = &f1.body {
            b.stmts.iter().take_while(|s| matches!(s, Stmt::Let(_))).filter_map(|s| {
                if let Stmt::Let(d) = s {
                    if let Pattern::Ident { name, .. } = &d.pattern {
                        Some(name.clone())
                    } else { None }
                } else { None }
            }).collect()
        } else { vec![] };
        let names2: Vec<String> = if let FnBody::Block(b) = &f2.body {
            b.stmts.iter().take_while(|s| matches!(s, Stmt::Let(_))).filter_map(|s| {
                if let Stmt::Let(d) = s {
                    if let Pattern::Ident { name, .. } = &d.pattern {
                        Some(name.clone())
                    } else { None }
                } else { None }
            }).collect()
        } else { vec![] };
        assert_eq!(names1, names2);
        // Verify alphabetical: _at_a, _at_b, _at_c.
        assert_eq!(names1, vec!["_at_a", "_at_b", "_at_c"]);
    }
}
