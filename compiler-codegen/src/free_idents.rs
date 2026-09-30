//! Scope-aware free-name collection over the AST, shared by the checker and the emitter
//! (moved out of `codegen/emit_c.rs`: the D184 init-order graph in `types::check_module_init_cycles`
//! and the emitter's `nova_consts_init` order must read ONE collector, or the two can disagree).

use crate::ast::*;
use std::cell::RefCell;
use std::collections::HashSet;

thread_local! {
    /// #1397: names the collector saw read as BOUND while `frame_reads` runs.
    static BOUND_READS: RefCell<Option<HashSet<String>>> = const { RefCell::new(None) };
}

/// #1397: every name a body reads, split into (read-as-bound, read-as-free); `params` are bound in it and `walk`
/// runs the collector over the body. The emitter's local frames (`codegen/local_frames.rs`) are built from this.
pub fn frame_reads(params: &[&str], walk: impl FnOnce(&mut HashSet<String>, &mut HashSet<String>)) -> (HashSet<String>, HashSet<String>) {
    let mut bound: HashSet<String> = params.iter().map(|p| p.to_string()).collect();
    let mut free = HashSet::new();
    BOUND_READS.with(|r| *r.borrow_mut() = Some(HashSet::new()));
    walk(&mut bound, &mut free);
    (BOUND_READS.with(|r| r.borrow_mut().take()).unwrap_or_default(), free)
}

// ─────────────────────────────────────────────────────────────────────
// Plan 62.D bis-1 (2026-05-18): scope-aware free-variable collector.
// The original `collect_free_idents` is misnamed — it returns ALL
// identifiers in an expression, including ones bound by inner `let`s,
// pattern matches, and nested lambdas. Used as-is for closure-capture
// analysis (emit_lambda line ~16670), the resulting filter
// (`var_types.contains_key`) incorrectly treats inner-shadowed names as
// captures from the outer scope when those names ALSO happen to be in
// `var_types` (e.g. `s` from a prior `let s = ...` in another function
// body — `var_types` is a global HashMap that doesn't reset per-fn).
//
// The fix: track inner bindings (let-statements, lambda params, match
// patterns) and exclude them from the free set. We use a `bound` HashSet
// that's pushed/popped as we descend.
//
// Production semantics: in a sequence of `let`s, the lexical scope of
// `let x = ...` covers the rest of the block. So binding order matters:
// `let s = ...; let f = || s + 1` — `s` IS free in `f`'s body. But
// `let f = || { let s = ...; s + 1 }` — `s` is locally bound, NOT free.
//
// The collector descends through blocks, growing `bound` as it visits
// each `let` BEFORE visiting subsequent statements (matches lexical scope).
pub fn collect_truly_free_idents(
    expr: &Expr,
    bound: &mut HashSet<String>,
    out: &mut HashSet<String>,
) {
    match &expr.kind {
        ExprKind::Ident(n) => {
            if !bound.contains(n) {
                out.insert(n.clone());
            } else {
                BOUND_READS.with(|r| if let Some(set) = r.borrow_mut().as_mut() { set.insert(n.clone()); }); // #1397
            }
        }
        ExprKind::Path(parts) if parts.len() == 2 => { out.insert(format!("{}.{}", parts[0], parts[1])); } // D184 amend: `Type.NAME` read
        ExprKind::Binary { left, right, .. } => {
            collect_truly_free_idents(left, bound, out);
            collect_truly_free_idents(right, bound, out);
        }
        ExprKind::Unary { operand, .. } => {
            collect_truly_free_idents(operand, bound, out);
        }
        ExprKind::Call { func, args, .. } => {
            collect_truly_free_idents(func, bound, out);
            for a in args { collect_truly_free_idents(a.expr(), bound, out); }
        }
        ExprKind::Member { obj, .. } => {
            collect_truly_free_idents(obj, bound, out);
        }
        ExprKind::Index { obj, index } => {
            collect_truly_free_idents(obj, bound, out);
            collect_truly_free_idents(index, bound, out);
        }
        ExprKind::Block(b) => {
            collect_truly_free_idents_block(b, bound, out);
        }
        ExprKind::If { cond, then, else_, .. } => {
            collect_truly_free_idents(cond, bound, out);
            collect_truly_free_idents_block(then, bound, out);
            if let Some(e) = else_ {
                match e {
                    ElseBranch::Block(b) =>
                        collect_truly_free_idents_block(b, bound, out),
                    ElseBranch::If(ex) =>
                        collect_truly_free_idents(ex, bound, out),
                }
            }
        }
        ExprKind::Lambda { params, body, .. } => {
            // Inner lambda: its params shadow outer scope inside the body.
            // Snapshot bound, add params, recurse, restore.
            let saved: Vec<String> = params.iter()
                .filter_map(|p| if bound.insert(p.name.clone()) { Some(p.name.clone()) } else { None })
                .collect();
            collect_truly_free_idents(body, bound, out);
            for n in saved { bound.remove(&n); }
        }
        ExprKind::ClosureLight { params, body } => {
            let saved: Vec<String> = params.iter()
                .filter_map(|p| if bound.insert(p.name.clone()) { Some(p.name.clone()) } else { None })
                .collect();
            match body {
                crate::ast::ClosureBody::Expr(e) =>
                    collect_truly_free_idents(e, bound, out),
                crate::ast::ClosureBody::Block(b) =>
                    collect_truly_free_idents_block(b, bound, out),
            }
            for n in saved { bound.remove(&n); }
        }
        ExprKind::ClosureFull(c) => {
            let saved: Vec<String> = c.params.iter()
                .filter_map(|p| if bound.insert(p.name.clone()) { Some(p.name.clone()) } else { None })
                .collect();
            match &c.body {
                crate::ast::FnBody::Expr(e) =>
                    collect_truly_free_idents(e, bound, out),
                crate::ast::FnBody::Block(b) =>
                    collect_truly_free_idents_block(b, bound, out),
                crate::ast::FnBody::External => {}
            }
            for n in saved { bound.remove(&n); }
        }
        ExprKind::TupleLit(elems) => {
            for e in elems { collect_truly_free_idents(e, bound, out); }
        }
        ExprKind::Detach(b) | ExprKind::Blocking(b) => {
            collect_truly_free_idents_block(b, bound, out);
        }
        ExprKind::Supervised { body, cancel, deadline, on_timeout } => {
            collect_truly_free_idents_block(body, bound, out);
            if let Some(c) = cancel {
                collect_truly_free_idents(c, bound, out);
            }
            if let Some(dl) = deadline {
                collect_truly_free_idents(&dl.expr, bound, out);
            }
            if let Some(oh) = on_timeout {
                collect_truly_free_idents(oh, bound, out);
            }
        }
        ExprKind::Select { arms } => {
            for arm in arms {
                match &arm.op {
                    SelectOp::Recv { chan, .. } =>
                        collect_truly_free_idents(chan, bound, out),
                    SelectOp::Send { chan, value } => {
                        collect_truly_free_idents(chan, bound, out);
                        collect_truly_free_idents(value, bound, out);
                    }
                    SelectOp::Default => {}
                }
                if let Some(g) = &arm.guard {
                    collect_truly_free_idents(g, bound, out);
                }
                collect_truly_free_idents_block(&arm.body, bound, out);
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            collect_truly_free_idents(scrutinee, bound, out);
            for arm in arms {
                // Pattern bindings shadow the outer scope inside the arm body.
                let mut pat_binds: HashSet<String> = HashSet::new();
                collect_pattern_bindings(&arm.pattern, &mut pat_binds);
                let added: Vec<String> = pat_binds.iter()
                    .filter_map(|n| if bound.insert(n.clone()) { Some(n.clone()) } else { None })
                    .collect();
                if let Some(g) = &arm.guard {
                    collect_truly_free_idents(g, bound, out);
                }
                match &arm.body {
                    MatchArmBody::Expr(e) =>
                        collect_truly_free_idents(e, bound, out),
                    MatchArmBody::Block(b) =>
                        collect_truly_free_idents_block(b, bound, out),
                }
                for n in added { bound.remove(&n); }
            }
        }
        // Plan 153.2 gap B: control-flow arms were missing here, so any name
        // referenced ONLY inside a `while`/`for`/`loop`/`while let`/`if let`
        // body that is captured by an enclosing closure was never collected
        // as a free variable (fell through to `_ => {}`). This mirrors the
        // canonical complete visitor `collect_idents_expr`, but preserves
        // scope discipline (loop-var / pattern bindings shadow captures).
        ExprKind::While { cond, body, .. } => {
            collect_truly_free_idents(cond, bound, out);
            collect_truly_free_idents_block(body, bound, out);
        }
        ExprKind::Loop { body, .. } => {
            collect_truly_free_idents_block(body, bound, out);
        }
        ExprKind::For { pattern, iter, body, .. }
        | ExprKind::ParallelFor { pattern, iter, body, .. } => {
            // `iter` is evaluated in the OUTER scope (loop var not yet bound).
            collect_truly_free_idents(iter, bound, out);
            let mut pat_binds: HashSet<String> = HashSet::new();
            collect_pattern_bindings(pattern, &mut pat_binds);
            let added: Vec<String> = pat_binds.iter()
                .filter_map(|n| if bound.insert(n.clone()) { Some(n.clone()) } else { None })
                .collect();
            collect_truly_free_idents_block(body, bound, out);
            for n in added { bound.remove(&n); }
        }
        ExprKind::WhileLet { pattern, scrutinee, guard, body, .. } => {
            // scrutinee evaluated before the pattern binds.
            collect_truly_free_idents(scrutinee, bound, out);
            let mut pat_binds: HashSet<String> = HashSet::new();
            collect_pattern_bindings(pattern, &mut pat_binds);
            let added: Vec<String> = pat_binds.iter()
                .filter_map(|n| if bound.insert(n.clone()) { Some(n.clone()) } else { None })
                .collect();
            // guard sees the pattern bindings.
            if let Some(g) = guard {
                collect_truly_free_idents(g, bound, out);
            }
            collect_truly_free_idents_block(body, bound, out);
            for n in added { bound.remove(&n); }
        }
        ExprKind::IfLet { pattern, scrutinee, guard, then, else_ } => {
            // scrutinee evaluated before the pattern binds.
            collect_truly_free_idents(scrutinee, bound, out);
            let mut pat_binds: HashSet<String> = HashSet::new();
            collect_pattern_bindings(pattern, &mut pat_binds);
            let added: Vec<String> = pat_binds.iter()
                .filter_map(|n| if bound.insert(n.clone()) { Some(n.clone()) } else { None })
                .collect();
            // guard sees the pattern bindings.
            if let Some(g) = guard {
                collect_truly_free_idents(g, bound, out);
            }
            collect_truly_free_idents_block(then, bound, out);
            for n in added { bound.remove(&n); }
            // else branch does NOT see the pattern bindings.
            if let Some(e) = else_ {
                match e {
                    ElseBranch::Block(b) =>
                        collect_truly_free_idents_block(b, bound, out),
                    ElseBranch::If(ex) =>
                        collect_truly_free_idents(ex, bound, out),
                }
            }
        }
        // Remaining sub-expr-bearing arms (robustness: future captures inside
        // these constructs are now collected). No new bindings introduced.
        ExprKind::Try(e) | ExprKind::Bang(e) | ExprKind::Throw(e)
        | ExprKind::Spawn(e) | ExprKind::As(e, _) | ExprKind::Is(e, _) => {
            collect_truly_free_idents(e, bound, out);
        }
        ExprKind::Coalesce(l, r) => {
            collect_truly_free_idents(l, bound, out);
            collect_truly_free_idents(r, bound, out);
        }
        ExprKind::TurboFish { base, .. } => {
            collect_truly_free_idents(base, bound, out);
        }
        ExprKind::Range { start, end, .. } => {
            if let Some(s) = start { collect_truly_free_idents(s, bound, out); }
            if let Some(e) = end { collect_truly_free_idents(e, bound, out); }
        }
        ExprKind::ArrayLit(elems) => {
            for elem in elems {
                match elem {
                    ArrayElem::Item(x) | ArrayElem::Spread(x) =>
                        collect_truly_free_idents(x, bound, out),
                }
            }
        }
        ExprKind::MapLit { elems, .. } => {
            for me in elems {
                match me {
                    crate::ast::MapElem::Pair(k, v) => {
                        collect_truly_free_idents(k, bound, out);
                        collect_truly_free_idents(v, bound, out);
                    }
                    crate::ast::MapElem::Spread(e) =>
                        collect_truly_free_idents(e, bound, out),
                }
            }
        }
        ExprKind::RecordLit { fields, .. } => {
            // Spread `...expr` is encoded as a field with is_spread=true and
            // value=Some(expr), so recursing every f.value covers it.
            for f in fields {
                if let Some(v) = &f.value { collect_truly_free_idents(v, bound, out); }
            }
        }
        ExprKind::With { bindings, body } => {
            for b in bindings { collect_truly_free_idents(&b.handler, bound, out); }
            collect_truly_free_idents_block(body, bound, out);
        }
        ExprKind::Interrupt(Some(v)) => {
            collect_truly_free_idents(v, bound, out);
        }
        // Владелец 2026-07-21 (найдено при str-concat-lint канонизации,
        // [M-str-interp-closure-capture-miss]): та же дыра, что и в
        // `collect_idents_expr` выше (см. её комментарий) — `${expr}`
        // внутри interpolated-string не обходился, идентификатор,
        // упомянутый ТОЛЬКО там, не попадал в closure free-var/capture
        // set → C codegen "use of undeclared identifier". Это ГЛАВНЫЙ
        // путь для `flat_map(|x| "...${captured}...")`-формы (emit_lambda,
        // не emit_spawn) — репро: `resolve_addr` (examples/flagship/
        // aggregator/src/main.nv).
        ExprKind::InterpolatedStr { parts } => {
            for p in parts {
                if let crate::ast::InterpStrPart::Expr { expr, .. } = p {
                    collect_truly_free_idents(expr, bound, out);
                }
            }
        }
        _ => {}
    }
}

pub fn collect_truly_free_idents_block(
    block: &Block,
    bound: &mut HashSet<String>,
    out: &mut HashSet<String>,
) {
    // Lexical scope: each `let x = ...` binds `x` for SUBSEQUENT stmts/trailing.
    // We collect inserted names and pop them all on block exit (block-local).
    let mut added: Vec<String> = Vec::new();
    for s in &block.stmts {
        match s {
            Stmt::Let(d) => {
                // Value expression evaluated in scope BEFORE binding x.
                collect_truly_free_idents(&d.value, bound, out);
                // After this let, x is bound for the rest of the block.
                let mut pat_binds: HashSet<String> = HashSet::new();
                collect_pattern_bindings(&d.pattern, &mut pat_binds);
                for n in pat_binds {
                    if bound.insert(n.clone()) {
                        added.push(n);
                    }
                }
            }
            Stmt::Assign { target, value, .. } => {
                collect_truly_free_idents(target, bound, out);
                collect_truly_free_idents(value, bound, out);
            }
            Stmt::Expr(e) =>
                collect_truly_free_idents(e, bound, out),
            Stmt::Return { value: Some(e), .. } =>
                collect_truly_free_idents(e, bound, out),
            _ => {}
        }
    }
    if let Some(t) = &block.trailing {
        collect_truly_free_idents(t, bound, out);
    }
    // Pop block-local bindings.
    for n in added { bound.remove(&n); }
}

/// Collect names introduced by a pattern (used by closure free-var collector
/// and by future scope analyses). Recursively visits sub-patterns; for `Or`,
/// takes the union (any alternative's bindings count as introduced). Wildcard,
/// literals, and unit-variants introduce nothing.
pub fn collect_pattern_bindings(pat: &Pattern, out: &mut HashSet<String>) {
    match pat {
        Pattern::Wildcard(_) | Pattern::Literal(..) => {}
        Pattern::Ident { name, .. } => { out.insert(name.clone()); }
        Pattern::Binding { name, inner, .. } => {
            out.insert(name.clone());
            collect_pattern_bindings(inner, out);
        }
        Pattern::Or { alternatives, .. } => {
            for alt in alternatives {
                collect_pattern_bindings(alt, out);
            }
        }
        Pattern::Variant { kind, .. } => {
            match kind {
                VariantPatternKind::Tuple { patterns, .. } => {
                    for p in patterns { collect_pattern_bindings(p, out); }
                }
                VariantPatternKind::Unit => {}
            }
        }
        Pattern::Record { fields, .. } => {
            for f in fields {
                if let Some(inner) = &f.pattern {
                    collect_pattern_bindings(inner, out);
                } else {
                    // shorthand `{ name }` — binds `name`.
                    out.insert(f.name.clone());
                }
            }
        }
        Pattern::Tuple(pats, _) => {
            for p in pats { collect_pattern_bindings(p, out); }
        }
        Pattern::Array { elems, .. } => {
            for el in elems {
                match el {
                    ArrayPatternElem::Item(p) => collect_pattern_bindings(p, out),
                    ArrayPatternElem::Rest => {}
                    ArrayPatternElem::RestBind(name) => { out.insert(name.clone()); }
                }
            }
        }
    }
}
