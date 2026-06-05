// SPDX-License-Identifier: MIT OR Apache-2.0
//! Plan 123.4.4 (2026-06-04): Codegen fluent-chain root-temp pre-pass.
//!
//! Closes `[M-123.4.4-codegen-fluent-chain-root-temp]`.
//!
//! ## Problem
//!
//! Fluent-chain expressions like `@buf.push(a).push(b).push(c)` lower
//! through `emit_c.rs` builtin-method dispatch (line 19474 for `push`):
//! each chain level recursively calls `emit_expr(obj)` to get the
//! receiver C-string, then emits its mutation statement using that
//! string, and returns the same string for the next chain level
//! ([Plan 91.7 / D181](fluent-`@`-return)).
//!
//! Result: `nova_self->buf` appears in C output **N times** для a
//! depth-N chain. Even though field_cache (V1 ro/mut) could cache
//! `@buf`, the chain pattern has only **one** AST `Member{SelfAccess,
//! buf}` node — field cache can't help.
//!
//! ## Fix
//!
//! AST pre-pass adjacent to `callnorm`: detect fluent chains of depth
//! ≥ 2 where each method is in a hard-coded set of known-fluent builtin
//! methods (push/append/extend_from/copy_from/insert/write_*/clear/
//! reserve), AND the root receiver is `Member{SelfAccess, F}` (the
//! common WriteBuffer/StringBuilder/[]T pattern). Rewrite к Block:
//!
//! ```nova
//! @buf.push(a).push(b).push(c)
//! // ↓
//! { let _chain_root_<N> = @buf; _chain_root_<N>.push(a);
//!   _chain_root_<N>.push(b); _chain_root_<N>.push(c); _chain_root_<N> }
//! ```
//!
//! After this rewrite, the codegen recursion emits the receiver
//! C-string ONCE для the let-binding, then `_chain_root_<N>` is emitted
//! как a simple Ident lookup для each subsequent method.
//!
//! ## Safety scope
//!
//! Restrict to **reference-typed receivers**: `_chain_root = @F` is a
//! pointer copy для `[]T`/`String`/`StringBuilder`/etc. Mutations
//! through `_chain_root` and `@F` reach the same heap object, so
//! semantics preserved. **Value-type fields not handled** — would
//! require TypeDecl integration (V2 followup).
//!
//! The V1 hard-coded fluent-method whitelist covers all known stdlib
//! fluent builders + array operations. User-defined `-> @` methods on
//! reference-typed receivers will be covered by V2 (TypeDecl-driven).

use crate::ast::*;

/// Plan 123.4.4 (V1): hard-coded list of known-fluent builtin methods
/// that return `@` (the receiver) and operate на reference-typed
/// receivers. Chains of these methods on `@F` receivers are safe к
/// hoist the root к a temp binding without changing semantics.
const FLUENT_BUILTIN_METHODS: &[&str] = &[
    // []T core mutators (Plan 90 / D141)
    "push", "append", "extend_from", "copy_from", "insert",
    "reserve", "fill", "clear", "extend_zero", "append_zero",
    // WriteBuffer / StringBuilder write-family (Plan 91.12 / Plan 109)
    "write_byte", "write_bytes", "write_zero", "write_char", "write_str",
    "write_u8", "write_i8",
    "write_u16_le", "write_u16_be", "write_i16_le", "write_i16_be",
    "write_u32_le", "write_u32_be", "write_i32_le", "write_i32_be",
    "write_u64_le", "write_u64_be", "write_i64_le", "write_i64_be",
    "write_f32_le", "write_f32_be", "write_f64_le", "write_f64_be",
];

fn is_fluent_builtin_method(name: &str) -> bool {
    FLUENT_BUILTIN_METHODS.contains(&name)
}

/// Plan 123.4.4 (V1): public entry-point. Walks every fn body's Expr
/// tree, normalizing fluent chains. Idempotent — calling twice produces
/// same output.
pub fn normalize_chains_module(module: &mut Module) {
    let mut counter = ChainCounter { next: 0 };
    for item in &mut module.items {
        normalize_chains_item(item, &mut counter);
    }
    for pf in &mut module.peer_files {
        for item in &mut pf.items_here {
            normalize_chains_item(item, &mut counter);
        }
    }
}

/// Monotonic counter ensuring unique chain-root temp names across whole
/// module (avoids cross-fn shadow conflicts при later passes).
struct ChainCounter {
    next: usize,
}

impl ChainCounter {
    fn alloc(&mut self) -> usize {
        let n = self.next;
        self.next += 1;
        n
    }
}

fn normalize_chains_item(item: &mut Item, counter: &mut ChainCounter) {
    if let Item::Fn(f) = item {
        normalize_chains_fn(f, counter);
    }
}

fn normalize_chains_fn(f: &mut FnDecl, counter: &mut ChainCounter) {
    match &mut f.body {
        FnBody::Block(b) => normalize_chains_block(b, counter),
        FnBody::Expr(e) => normalize_chains_expr(e, counter),
        FnBody::External => {}
    }
}

fn normalize_chains_block(b: &mut Block, counter: &mut ChainCounter) {
    for s in &mut b.stmts {
        normalize_chains_stmt(s, counter);
    }
    if let Some(t) = &mut b.trailing {
        normalize_chains_expr(t, counter);
    }
}

fn normalize_chains_stmt(s: &mut Stmt, counter: &mut ChainCounter) {
    match s {
        Stmt::Let(d) => normalize_chains_expr(&mut d.value, counter),
        Stmt::Const(d) => normalize_chains_expr(&mut d.value, counter),
        Stmt::Expr(e) => normalize_chains_expr(e, counter),
        Stmt::Assign { target, value, .. } => {
            normalize_chains_expr(target, counter);
            normalize_chains_expr(value, counter);
        }
        Stmt::Return { value, .. } => {
            if let Some(v) = value { normalize_chains_expr(v, counter); }
        }
        Stmt::Throw { value, .. } => normalize_chains_expr(value, counter),
        Stmt::Defer { body, .. } | Stmt::ErrDefer { body, .. }
        | Stmt::OkDefer { body, .. } | Stmt::DeferWithResult { body, .. } => {
            normalize_chains_expr(body, counter);
        }
        Stmt::ConsumeScope { init, body, .. } => {
            normalize_chains_expr(init, counter);
            normalize_chains_block(body, counter);
        }
        Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => {
            normalize_chains_expr(expr, counter);
        }
        Stmt::Break(_) | Stmt::Continue(_)
        | Stmt::Apply { .. } | Stmt::Calc { .. } | Stmt::Reveal { .. } => {}
    }
}

fn normalize_chains_expr(e: &mut Expr, counter: &mut ChainCounter) {
    // Top-down: check if THIS Expr is the outermost frame of a fluent
    // chain. If so, wrap in Block. Otherwise descend into children.
    // Top-down ordering ensures the outermost chain captures all
    // depth-N frames at once (bottom-up would rewrite inner frames first,
    // breaking the outer extractor's left-deep Call.func walk).
    if let Some(chain_info) = try_extract_outer_fluent_chain(e) {
        if chain_info.depth >= 2 {
            *e = build_chain_block(chain_info, counter);
            // Descend into the wrapped Block — its stmts/trailing may
            // contain further chains (e.g., method args themselves
            // hosting chains).
            normalize_chains_expr_children(e, counter);
            return;
        }
    }
    // Not the outermost frame of a chain — descend into children.
    normalize_chains_expr_children(e, counter);
}

fn normalize_chains_expr_children(e: &mut Expr, counter: &mut ChainCounter) {
    match &mut e.kind {
        ExprKind::Lambda { body, .. } => normalize_chains_expr(body, counter),
        ExprKind::ClosureLight { body, .. } => match body {
            ClosureBody::Expr(e) => normalize_chains_expr(e, counter),
            ClosureBody::Block(b) => normalize_chains_block(b, counter),
        },
        ExprKind::ClosureFull(sb) => match &mut sb.body {
            FnBody::Expr(e) => normalize_chains_expr(e, counter),
            FnBody::Block(b) => normalize_chains_block(b, counter),
            FnBody::External => {}
        },
        ExprKind::Block(b) => normalize_chains_block(b, counter),
        ExprKind::If { cond, then, else_ } => {
            normalize_chains_expr(cond, counter);
            normalize_chains_block(then, counter);
            if let Some(eb) = else_ {
                normalize_chains_else(eb, counter);
            }
        }
        ExprKind::IfLet { scrutinee, then, else_, .. } => {
            normalize_chains_expr(scrutinee, counter);
            normalize_chains_block(then, counter);
            if let Some(eb) = else_ {
                normalize_chains_else(eb, counter);
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            normalize_chains_expr(scrutinee, counter);
            for arm in arms.iter_mut() {
                if let Some(g) = &mut arm.guard { normalize_chains_expr(g, counter); }
                match &mut arm.body {
                    MatchArmBody::Expr(e) => normalize_chains_expr(e, counter),
                    MatchArmBody::Block(b) => normalize_chains_block(b, counter),
                }
            }
        }
        ExprKind::For { iter, body, .. } | ExprKind::ParallelFor { iter, body, .. } => {
            normalize_chains_expr(iter, counter);
            normalize_chains_block(body, counter);
        }
        ExprKind::While { cond, body, .. } => {
            normalize_chains_expr(cond, counter);
            normalize_chains_block(body, counter);
        }
        ExprKind::WhileLet { scrutinee, body, .. } => {
            normalize_chains_expr(scrutinee, counter);
            normalize_chains_block(body, counter);
        }
        ExprKind::Loop { body, .. } => normalize_chains_block(body, counter),
        ExprKind::With { bindings, body } => {
            for wb in bindings.iter_mut() {
                normalize_chains_expr(&mut wb.handler, counter);
            }
            normalize_chains_block(body, counter);
        }
        ExprKind::Forbid { body, .. } | ExprKind::Realtime { body, .. }
        | ExprKind::Detach(body) | ExprKind::Blocking(body) =>
            normalize_chains_block(body, counter),
        ExprKind::Supervised { body, cancel } => {
            normalize_chains_block(body, counter);
            if let Some(c) = cancel { normalize_chains_expr(c, counter); }
        }
        ExprKind::Spawn(e) | ExprKind::Throw(e) => normalize_chains_expr(e, counter),
        ExprKind::Try(e) | ExprKind::Bang(e)
        | ExprKind::Member { obj: e, .. } | ExprKind::TurboFish { base: e, .. }
        | ExprKind::As(e, _) | ExprKind::Is(e, _)
        | ExprKind::Unary { operand: e, .. } => normalize_chains_expr(e, counter),
        ExprKind::Coalesce(a, b) | ExprKind::Binary { left: a, right: b, .. } => {
            normalize_chains_expr(a, counter);
            normalize_chains_expr(b, counter);
        }
        ExprKind::Index { obj, index } => {
            normalize_chains_expr(obj, counter);
            normalize_chains_expr(index, counter);
        }
        ExprKind::Call { func, args, trailing } => {
            normalize_chains_expr(func, counter);
            for arg in args.iter_mut() {
                let inner = match arg {
                    CallArg::Item(e) | CallArg::Spread(e) => e,
                    CallArg::Named { value, .. } => value,
                };
                normalize_chains_expr(inner, counter);
            }
            if let Some(t) = trailing {
                match t {
                    Trailing::Block(b) => normalize_chains_block(b, counter),
                    Trailing::Fn(sb) => match &mut sb.body {
                        FnBody::Expr(e) => normalize_chains_expr(e, counter),
                        FnBody::Block(b) => normalize_chains_block(b, counter),
                        FnBody::External => {}
                    },
                    Trailing::LegacyBlockWithParams(tb) =>
                        normalize_chains_block(&mut tb.body, counter),
                }
            }
        }
        ExprKind::ArrayLit(elems) => {
            for el in elems.iter_mut() {
                match el {
                    ArrayElem::Item(e) | ArrayElem::Spread(e) =>
                        normalize_chains_expr(e, counter),
                }
            }
        }
        ExprKind::MapLit { elems, .. } => {
            for el in elems.iter_mut() {
                match el {
                    MapElem::Pair(k, v) => {
                        normalize_chains_expr(k, counter);
                        normalize_chains_expr(v, counter);
                    }
                    MapElem::Spread(e) => normalize_chains_expr(e, counter),
                }
            }
        }
        ExprKind::RecordLit { fields, .. } => {
            for rf in fields.iter_mut() {
                if let Some(v) = &mut rf.value { normalize_chains_expr(v, counter); }
            }
        }
        ExprKind::TupleLit(elems) => {
            for el in elems.iter_mut() { normalize_chains_expr(el, counter); }
        }
        ExprKind::InterpolatedStr { parts } => {
            for p in parts.iter_mut() {
                if let InterpStrPart::Expr { expr: e, spec: _ } = p { normalize_chains_expr(e, counter); }
            }
        }
        ExprKind::TaggedTemplate { tag, args, .. } => {
            normalize_chains_expr(tag, counter);
            for a in args.iter_mut() { normalize_chains_expr(a, counter); }
        }
        ExprKind::Range { start, end, .. } => {
            if let Some(s) = start { normalize_chains_expr(s, counter); }
            if let Some(e) = end { normalize_chains_expr(e, counter); }
        }
        ExprKind::Forall { range, body, .. } | ExprKind::Exists { range, body, .. } => {
            normalize_chains_expr(range, counter);
            normalize_chains_expr(body, counter);
        }
        ExprKind::Interrupt(opt) => {
            if let Some(e) = opt { normalize_chains_expr(e, counter); }
        }
        // Leaf / literal — nothing к descend into.
        _ => {}
    }
}

fn normalize_chains_else(eb: &mut ElseBranch, counter: &mut ChainCounter) {
    match eb {
        ElseBranch::Block(b) => normalize_chains_block(b, counter),
        ElseBranch::If(e) => normalize_chains_expr(e, counter),
    }
}

/// Plan 123.4.4 (V1): one method-call frame in a detected chain.
struct ChainFrame {
    method: String,
    args: Vec<CallArg>,
    trailing: Option<Trailing>,
    span: crate::diag::Span,
}

/// Plan 123.4.4 (V1): extracted fluent chain — chain depth (≥ 1) +
/// frames в outermost-first order, plus the root receiver expression
/// (which we'll bind к a temp).
struct FluentChain {
    depth: usize,
    /// Frames в LEFT-DEEP order: first = innermost (root call), last =
    /// outermost. Each frame's receiver is the previous frame's result
    /// (или the root receiver для frame[0]).
    frames: Vec<ChainFrame>,
    /// Root receiver — `@F` `Member{SelfAccess, F}` pattern (the
    /// hoist target).
    root: Expr,
    /// Root receiver field name (for `_chain_root_<N>_<field>` naming
    /// improvement и future TypeDecl integration).
    root_field: String,
    /// Span used для inserted Block + Let stmts.
    outer_span: crate::diag::Span,
}

/// Plan 123.4.4 (V1): detect-and-extract a fluent chain rooted at this
/// Expr. Returns `Some(chain)` если e is the OUTERMOST Call of a chain
/// matching:
/// - Chain depth ≥ 2 (else не worth hoisting).
/// - Each method name в `FLUENT_BUILTIN_METHODS`.
/// - Root receiver is `Member{SelfAccess, F}` (safe-hoist pattern).
fn try_extract_outer_fluent_chain(e: &Expr) -> Option<FluentChain> {
    let mut frames: Vec<ChainFrame> = Vec::new();
    let mut cur = e;
    let outer_span = e.span;
    // Walk down chain, accumulating frames в outer-to-inner order; we'll
    // reverse к inner-to-outer at the end.
    loop {
        if let ExprKind::Call { func, args, trailing } = &cur.kind {
            if let ExprKind::Member { obj, name } = &func.kind {
                if !is_fluent_builtin_method(name) {
                    // Method not in fluent set — abort (we only hoist
                    // chains of known-safe fluent calls).
                    return None;
                }
                frames.push(ChainFrame {
                    method: name.clone(),
                    args: args.clone(),
                    trailing: trailing.clone(),
                    span: cur.span,
                });
                cur = obj;
                continue;
            }
        }
        break;
    }
    // cur is now the root receiver. Must be Member{SelfAccess, F}.
    let (root_field, root) = match &cur.kind {
        ExprKind::Member { obj, name } if matches!(obj.kind, ExprKind::SelfAccess) => {
            (name.clone(), cur.clone())
        }
        _ => return None, // not safe-hoistable root pattern
    };
    if frames.len() < 2 {
        return None; // single-method call — не a chain
    }
    // Frames were accumulated outer-to-inner during walk-down. Reverse к
    // get inner-to-outer execution order.
    frames.reverse();
    Some(FluentChain {
        depth: frames.len(),
        frames,
        root,
        root_field,
        outer_span,
    })
}

/// Plan 123.4.4 (V1): rewrite a detected fluent chain в Block с temp.
///
/// `e.m1(a).m2(b).m3(c)` (where root = @F)  becomes:
///
/// ```nova
/// {
///   let _chain_root_<N>_<field> = @F;
///   _chain_root_<N>_<field>.m1(a);
///   _chain_root_<N>_<field>.m2(b);
///   _chain_root_<N>_<field>.m3(c);
///   _chain_root_<N>_<field>
/// }
/// ```
///
/// The trailing `_chain_root_<N>_<field>` preserves chain's value-as-
/// receiver semantics — caller sees the (mutated) root binding.
fn build_chain_block(chain: FluentChain, counter: &mut ChainCounter) -> Expr {
    let n = counter.alloc();
    let local_name = format!("_chain_root_{}_{}", n, chain.root_field);
    let span = chain.outer_span;
    // Let-stmt: `let <local> = <root>;`
    let let_stmt = Stmt::Let(LetDecl {
        mutable: false,
        pattern: Pattern::Ident {
            name: local_name.clone(),
            span,
            is_mut: false,
        },
        ty: None,
        value: chain.root,
        span,
        is_ghost: false,
        consume: false,
    });
    let mut stmts: Vec<Stmt> = vec![let_stmt];
    // Each chain frame becomes an Expr-stmt invoking method на the
    // local binding.
    for frame in &chain.frames {
        let recv = Expr {
            kind: ExprKind::Ident(local_name.clone()),
            span: frame.span,
        };
        let call = Expr {
            kind: ExprKind::Call {
                func: Box::new(Expr {
                    kind: ExprKind::Member {
                        obj: Box::new(recv),
                        name: frame.method.clone(),
                    },
                    span: frame.span,
                }),
                args: frame.args.clone(),
                trailing: frame.trailing.clone(),
            },
            span: frame.span,
        };
        stmts.push(Stmt::Expr(call));
    }
    // Trailing — re-bind the local back as the Block's value-expression
    // (preserves chain's "return receiver" semantics for D181 fluent
    // `@`-return chains).
    let trailing = Some(Box::new(Expr {
        kind: ExprKind::Ident(local_name.clone()),
        span,
    }));
    Expr {
        kind: ExprKind::Block(Block {
            stmts,
            trailing,
            span,
            is_unsafe: false,
        }),
        span,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;

    fn run(src: &str) -> Module {
        let mut m = parse(src).expect("parse");
        normalize_chains_module(&mut m);
        m
    }

    fn find_fn<'a>(m: &'a Module, name: &str) -> &'a FnDecl {
        for item in &m.items {
            if let Item::Fn(f) = item {
                if f.name == name { return f; }
            }
        }
        panic!("fn {} not found", name)
    }

    /// Test helper: count nested `Member{SelfAccess, F}` reads of `@<field>`
    /// в a fn body. Before chain-norm pass, a depth-N chain has only 1
    /// such read. After, also 1 (in the let-binding) + N idents.
    fn count_self_member_reads(f: &FnDecl, field: &str) -> usize {
        let mut count = 0;
        fn walk_block(b: &Block, field: &str, count: &mut usize) {
            for s in &b.stmts { walk_stmt(s, field, count); }
            if let Some(t) = &b.trailing { walk_expr(t, field, count); }
        }
        fn walk_stmt(s: &Stmt, field: &str, count: &mut usize) {
            match s {
                Stmt::Let(d) => walk_expr(&d.value, field, count),
                Stmt::Const(d) => walk_expr(&d.value, field, count),
                Stmt::Expr(e) => walk_expr(e, field, count),
                Stmt::Assign { target, value, .. } => {
                    walk_expr(target, field, count);
                    walk_expr(value, field, count);
                }
                Stmt::Return { value, .. } => {
                    if let Some(v) = value { walk_expr(v, field, count); }
                }
                Stmt::Throw { value, .. } => walk_expr(value, field, count),
                _ => {}
            }
        }
        fn walk_expr(e: &Expr, field: &str, count: &mut usize) {
            if let ExprKind::Member { obj, name } = &e.kind {
                if matches!(obj.kind, ExprKind::SelfAccess) && name == field {
                    *count += 1;
                }
                walk_expr(obj, field, count);
            }
            match &e.kind {
                ExprKind::Block(b) => walk_block(b, field, count),
                ExprKind::Call { func, args, .. } => {
                    walk_expr(func, field, count);
                    for arg in args {
                        let inner = match arg {
                            CallArg::Item(e) | CallArg::Spread(e) => e,
                            CallArg::Named { value, .. } => value,
                        };
                        walk_expr(inner, field, count);
                    }
                }
                ExprKind::Binary { left, right, .. } => {
                    walk_expr(left, field, count);
                    walk_expr(right, field, count);
                }
                ExprKind::Member { obj, .. } => walk_expr(obj, field, count),
                _ => {}
            }
        }
        if let FnBody::Block(b) = &f.body { walk_block(b, field, &mut count); }
        count
    }

    fn count_chain_root_lets(f: &FnDecl) -> usize {
        let mut c: usize = 0;
        fn walk_block(b: &Block, c: &mut usize) {
            for s in &b.stmts {
                if let Stmt::Let(d) = s {
                    if let Pattern::Ident { name, .. } = &d.pattern {
                        if name.starts_with("_chain_root_") { *c += 1; }
                    }
                }
                walk_stmt(s, c);
            }
            if let Some(t) = &b.trailing { walk_expr(t, c); }
        }
        fn walk_stmt(s: &Stmt, c: &mut usize) {
            match s {
                Stmt::Let(d) => walk_expr(&d.value, c),
                Stmt::Expr(e) => walk_expr(e, c),
                _ => {}
            }
        }
        fn walk_expr(e: &Expr, c: &mut usize) {
            match &e.kind {
                ExprKind::Block(b) => walk_block(b, c),
                ExprKind::If { then, else_, .. } | ExprKind::IfLet { then, else_, .. } => {
                    walk_block(then, c);
                    if let Some(eb) = else_ {
                        match eb {
                            ElseBranch::Block(b) => walk_block(b, c),
                            ElseBranch::If(e) => walk_expr(e, c),
                        }
                    }
                }
                ExprKind::Match { arms, .. } => {
                    for arm in arms {
                        match &arm.body {
                            MatchArmBody::Expr(e) => walk_expr(e, c),
                            MatchArmBody::Block(b) => walk_block(b, c),
                        }
                    }
                }
                ExprKind::For { body, .. } | ExprKind::ParallelFor { body, .. } => walk_block(body, c),
                ExprKind::While { body, .. } | ExprKind::WhileLet { body, .. }
                | ExprKind::Loop { body, .. } => walk_block(body, c),
                _ => {}
            }
        }
        if let FnBody::Block(b) = &f.body { walk_block(b, &mut c); }
        else if let FnBody::Expr(e) = &f.body { walk_expr(e, &mut c); }
        c
    }

    /// V123.4.4.1 positive: depth-2 chain `@buf.push(a).push(b)` gets
    /// wrapped в Block-temp.
    #[test]
    fn chain_norm_depth_2_push() {
        let src = r#"
module testmod.cn_depth2
type Buf { mut buf []int }
fn Buf mut @do(a int, b int) -> Buf {
    @buf.push(a).push(b)
    @
}
"#;
        let m = run(src);
        let f = find_fn(&m, "do");
        assert_eq!(count_chain_root_lets(f), 1,
            "expected 1 _chain_root_* let for depth-2 chain");
    }

    /// V123.4.4.2 positive: depth-3 chain gets ONE temp (not three).
    #[test]
    fn chain_norm_depth_3_push() {
        let src = r#"
module testmod.cn_depth3
type Buf { mut buf []int }
fn Buf mut @triple(a int, b int, c int) -> Buf {
    @buf.push(a).push(b).push(c)
    @
}
"#;
        let m = run(src);
        let f = find_fn(&m, "triple");
        assert_eq!(count_chain_root_lets(f), 1);
    }

    /// V123.4.4.3 negative: depth-1 (single call) NOT wrapped.
    #[test]
    fn chain_norm_depth_1_not_wrapped() {
        let src = r#"
module testmod.cn_depth1
type Buf { mut buf []int }
fn Buf mut @one(a int) -> Buf {
    @buf.push(a)
    @
}
"#;
        let m = run(src);
        let f = find_fn(&m, "one");
        assert_eq!(count_chain_root_lets(f), 0);
    }

    /// V123.4.4.4 negative: non-fluent method (e.g. `len`) chain NOT
    /// wrapped — отказ от list.
    #[test]
    fn chain_norm_non_fluent_method_not_wrapped() {
        let src = r#"
module testmod.cn_non_fluent
type Buf { mut buf []int }
fn Buf @count() -> int {
    @buf.len()
}
"#;
        let m = run(src);
        let f = find_fn(&m, "count");
        assert_eq!(count_chain_root_lets(f), 0,
            "single len() call shouldn't trigger chain rewrite");
    }

    /// V123.4.4.5 negative: chain rooted на non-self receiver NOT
    /// wrapped (e.g. `local.push().push()`).
    #[test]
    fn chain_norm_non_self_root_not_wrapped() {
        let src = r#"
module testmod.cn_non_self_root
fn process(mut v []int, a int, b int) -> () {
    v.push(a).push(b)
}
"#;
        let m = run(src);
        let f = find_fn(&m, "process");
        assert_eq!(count_chain_root_lets(f), 0,
            "non-self chain root shouldn't be wrapped");
    }

    /// V123.4.4.6 positive: AFTER rewrite, `@buf` Member-SelfAccess
    /// reads count = 1 (only в let-binding), regardless of chain
    /// depth.
    #[test]
    fn chain_norm_reads_self_member_once() {
        let src = r#"
module testmod.cn_member_once
type Buf { mut buf []int }
fn Buf mut @cycle(a int, b int, c int, d int) -> Buf {
    @buf.push(a).push(b).push(c).push(d)
    @
}
"#;
        let m = run(src);
        let f = find_fn(&m, "cycle");
        // After rewrite: `let _chain_root_0_buf = @buf;`. Single
        // `@buf` Member-SelfAccess read.
        assert_eq!(count_self_member_reads(f, "buf"), 1,
            "expected 1 @buf read после chain rewrite (was N=4 before)");
    }

    /// V123.4.4.7 positive: trailing expression preserves chain's
    /// receiver-return semantics — Block.trailing == Ident(temp).
    #[test]
    fn chain_norm_trailing_returns_receiver() {
        let src = r#"
module testmod.cn_trailing
type Buf { mut buf []int }
fn Buf mut @do(a int, b int) -> Buf {
    @buf.push(a).push(b)
    @
}
"#;
        let m = run(src);
        let f = find_fn(&m, "do");
        if let FnBody::Block(b) = &f.body {
            // Find the chain expr (it's stmt[0] = Stmt::Expr(Block{...})).
            if let Stmt::Expr(e) = &b.stmts[0] {
                if let ExprKind::Block(blk) = &e.kind {
                    let trailing = blk.trailing.as_ref().expect("trailing");
                    if let ExprKind::Ident(name) = &trailing.kind {
                        assert!(name.starts_with("_chain_root_"),
                            "trailing should be chain-root ident; got {}", name);
                    } else {
                        panic!("expected Ident trailing; got {:?}", trailing.kind);
                    }
                }
            }
        }
    }

    /// V123.4.4.8 unit: `is_fluent_builtin_method` recognizes expected
    /// methods и rejects others.
    #[test]
    fn chain_norm_fluent_method_recognition() {
        assert!(is_fluent_builtin_method("push"));
        assert!(is_fluent_builtin_method("append"));
        assert!(is_fluent_builtin_method("write_byte"));
        assert!(is_fluent_builtin_method("write_u32_le"));
        assert!(!is_fluent_builtin_method("len"));
        assert!(!is_fluent_builtin_method("pop"));
        assert!(!is_fluent_builtin_method("get"));
        assert!(!is_fluent_builtin_method(""));
    }

    /// V123.4.4.9 positive: nested chains (inner if-then containing
    /// own chain) handled bottom-up.
    #[test]
    fn chain_norm_nested_in_if_then() {
        let src = r#"
module testmod.cn_nested_if
type Buf { mut buf []int }
fn Buf mut @do(cond bool, a int, b int) -> Buf {
    if cond {
        @buf.push(a).push(b)
    }
    @
}
"#;
        let m = run(src);
        let f = find_fn(&m, "do");
        // The if's then-block has the chain — should be wrapped.
        assert_eq!(count_chain_root_lets(f), 1,
            "expected nested chain inside if-then к wrap");
    }

    /// V123.4.4.10 positive: idempotency — running normalize_chains
    /// twice produces same output.
    #[test]
    fn chain_norm_idempotent() {
        let src = r#"
module testmod.cn_idem
type Buf { mut buf []int }
fn Buf mut @do(a int, b int) -> Buf {
    @buf.push(a).push(b)
    @
}
"#;
        let mut m = parse(src).expect("parse");
        normalize_chains_module(&mut m);
        let count1 = count_chain_root_lets(find_fn(&m, "do"));
        normalize_chains_module(&mut m);
        let count2 = count_chain_root_lets(find_fn(&m, "do"));
        assert_eq!(count1, count2,
            "second normalization shouldn't add chains");
    }
}
