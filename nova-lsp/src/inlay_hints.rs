//! Inlay hints — Plan 104.10 Ф.9 (BLOCK C).
//!
//! Two kinds of editor-inlined hints, both derived from **real** semantic
//! information (never a textual heuristic):
//!
//! - **Type hints** — for an un-annotated binding `ro x = expr` / `mut x = expr`
//!   the inferred type is shown as ` T` — a SPACE, not a colon — right after the
//!   variable name, because that is how the annotation is spelled in Nova
//!   (`ro x int = 5`, `consume s TcpStream`): the hint has to read as the code the
//!   user would have written. Owner's decision of 2026-07-23, implemented in
//!   `5f484c7b2`. This paragraph said `: T` until 2026-09-01 and was simply missed
//!   by that commit — as was the e2e fixture `f9_pos_inlay_hints_type_and_params`,
//!   which is how the drift surfaced (registry №854). The type
//!   comes from the Ф.2 `expr_types` map (the checker's inference for the
//!   initializer expression), so `ro x = 5` renders `x int` and
//!   `ro r = 0..10` renders `r Range`. An **already-annotated** binding
//!   (`ro x int = 5`) gets no hint — the type is already on screen. Which
//!   initializer shapes carry a type is exactly what the Ф.2 map records: the
//!   IDE resolve path does not run `number_exprs`, so the ExprId-keyed semantic
//!   channel (call returns, some index/member chains) is not joined and those
//!   bindings show no type hint — the pre-existing `[M-104.10-expr-types-coverage]`
//!   gap, not a defect here. Absence degrades gracefully (no wrong hint).
//!
//! - **Parameter name hints** — inside a call `foo(1, 2)` the callee's parameter
//!   names are shown before each positional argument (`foo(a: 1, b: 2)`), exactly
//!   like rust-analyzer / IntelliJ. The parameter names come from the resolved
//!   callee `FnDecl` (free function *or* value method `recv.m(…)`), matched by
//!   name + arity against the module AST. A redundant hint (`foo(count)` where the
//!   parameter is itself named `count`) is suppressed.
//!
//! Both kinds are individually toggleable via [`InlayHintConfig`] (default: both
//! **on**), read from the client's `initializationOptions` /
//! `workspace/didChangeConfiguration`. See `[M-104.10-inlay-config-granularity]`
//! for the residual granularity gap.
//!
//! # UTF-16 correctness
//!
//! Every hint position is produced by [`byte_offset_to_position`], which walks the
//! rope counting **UTF-16 code units** — so a hint after a multi-byte identifier
//! (`ro café = 1`) lands at the correct client column, not a byte offset.
//!
//! # Scope / provenance
//!
//! Only the **entry file's own** items are walked (`items_start..`), and every
//! emitted hint's anchoring `Span` is asserted to carry `MAIN_FILE_ID` before its
//! byte offset is converted against the current buffer — a span from a prepended
//! import (foreign `file_id`) is never mis-projected onto the current document.
//!
//! # Residual — [M-104.10-inlay-config-granularity]
//!
//! Config granularity is the two headline toggles (type hints, parameter hints)
//! plus a master `enable`. Finer rust-analyzer-style knobs (hints only for
//! literal arguments, hide-single-param, closure-return hints, max length,
//! chaining hints) are not yet exposed. Tracked in `simplifications.md` / backlog.

use ropey::Rope;
use tower_lsp::lsp_types::{InlayHint, InlayHintKind, InlayHintLabel, Position, Range};

use nova_codegen::ast::{
    ArrayElem, Block, CallArg, ClosureBody, ElseBranch, Expr, ExprKind, FnBody, FnDecl, Item,
    LetDecl, MatchArmBody, Pattern, Stmt, TypeRef,
};
use nova_codegen::diag::{Span, MAIN_FILE_ID};
use nova_codegen::types::ModuleEnv;

use crate::diagnostic_mapping::byte_offset_to_position;
use crate::provenance::ResolvedModule;
use crate::symbol::{find_fn_by_name, find_method_by_name, format_type_ref};

// ─────────────────────────────────────────────────────────────────────────────
// Config
// ─────────────────────────────────────────────────────────────────────────────

/// Which inlay-hint kinds to produce. Both default **on** (Plan Ф.9 criterion:
/// "оба вида включаемы, по умолчанию оба on").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InlayHintConfig {
    /// `ro x = expr` → `: T` type hints.
    pub type_hints: bool,
    /// `foo(1, 2)` → `a:`/`b:` parameter-name hints.
    pub parameter_hints: bool,
}

impl Default for InlayHintConfig {
    fn default() -> Self {
        Self { type_hints: true, parameter_hints: true }
    }
}

impl InlayHintConfig {
    /// Parse an `InlayHintConfig` from a client-supplied settings object
    /// (`initializationOptions` or the `settings` of a `didChangeConfiguration`).
    ///
    /// Recognised (all optional, all default to the current/`Default` value):
    /// - `nova.inlayHints.enable` (bool, master switch — `false` disables both)
    /// - `nova.inlayHints.typeHints` (bool)
    /// - `nova.inlayHints.parameterHints` (bool)
    ///
    /// The `nova` wrapper object is optional: a bare `inlayHints.*` at the root is
    /// also accepted (editors differ in how they scope section names).
    pub fn from_settings(v: &serde_json::Value) -> Self {
        let mut cfg = Self::default();
        let node = v
            .get("nova")
            .and_then(|n| n.get("inlayHints"))
            .or_else(|| v.get("inlayHints"));
        if let Some(node) = node {
            if let Some(b) = node.get("typeHints").and_then(|x| x.as_bool()) {
                cfg.type_hints = b;
            }
            if let Some(b) = node.get("parameterHints").and_then(|x| x.as_bool()) {
                cfg.parameter_hints = b;
            }
            // Master switch: `enable: false` overrides both to off.
            if node.get("enable").and_then(|x| x.as_bool()) == Some(false) {
                cfg.type_hints = false;
                cfg.parameter_hints = false;
            }
        }
        cfg
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Public API
// ─────────────────────────────────────────────────────────────────────────────

/// Compute inlay hints for the requested `range` against an already-resolved
/// module (Ф.1 cache) and its Ф.2 `expr_types`.
///
/// Only hints whose anchor position falls inside `range` are returned (the client
/// requests hints viewport-by-viewport). A parse failure / missing `env` degrades
/// gracefully: type hints need `expr_types` (skipped when absent); parameter hints
/// need only the AST and still work.
pub fn compute_inlay_hints_in(
    resolved: &ResolvedModule,
    src: &str,
    range: Range,
    cfg: InlayHintConfig,
) -> Vec<InlayHint> {
    let mut out = Vec::new();
    if !cfg.type_hints && !cfg.parameter_hints {
        return out;
    }
    let rope = Rope::from_str(src);
    let start = resolved.items_start.min(resolved.module.items.len());
    let mut cx = Ctx {
        module: &resolved.module,
        env: resolved.env.as_ref(),
        rope: &rope,
        src,
        range,
        cfg,
        out: &mut out,
    };
    for item in &resolved.module.items[start..] {
        cx.visit_item(item);
    }
    out
}

// ─────────────────────────────────────────────────────────────────────────────
// Walk
// ─────────────────────────────────────────────────────────────────────────────

struct Ctx<'a> {
    module: &'a nova_codegen::ast::Module,
    env: Option<&'a ModuleEnv>,
    rope: &'a Rope,
    src: &'a str,
    range: Range,
    cfg: InlayHintConfig,
    out: &'a mut Vec<InlayHint>,
}

/// A resolved parameter's identity for hint rendering.
struct ParamInfo {
    name: String,
    is_variadic: bool,
}

impl<'a> Ctx<'a> {
    fn visit_item(&mut self, item: &Item) {
        match item {
            Item::Fn(fd) => self.visit_fn(fd),
            Item::Test(td) => self.visit_block(&td.body),
            Item::Let(ld) => {
                self.emit_type_hint(ld);
                self.visit_expr(&ld.value);
            }
            Item::Const(cd) => self.visit_expr(&cd.value),
            _ => {}
        }
    }

    fn visit_fn(&mut self, fd: &FnDecl) {
        for p in &fd.params {
            if let Some(def) = &p.default {
                self.visit_expr(def);
            }
        }
        match &fd.body {
            FnBody::Block(b) => self.visit_block(b),
            FnBody::Expr(e) => self.visit_expr(e),
            FnBody::External => {}
        }
    }

    fn visit_block(&mut self, block: &Block) {
        for s in &block.stmts {
            self.visit_stmt(s);
        }
        if let Some(t) = &block.trailing {
            self.visit_expr(t);
        }
    }

    fn visit_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Let(ld) => {
                self.emit_type_hint(ld);
                self.visit_expr(&ld.value);
            }
            Stmt::Const(cd) => self.visit_expr(&cd.value),
            Stmt::Expr(e) => self.visit_expr(e),
            Stmt::Assign { target, value, .. } => {
                self.visit_expr(target);
                self.visit_expr(value);
            }
            Stmt::TupleAssign { lhs, rhs, .. } => {
                for e in lhs {
                    self.visit_expr(e);
                }
                for e in rhs {
                    self.visit_expr(e);
                }
            }
            Stmt::Return { value, .. } => {
                if let Some(e) = value {
                    self.visit_expr(e);
                }
            }
            Stmt::Throw { value, .. } => self.visit_expr(value),
            Stmt::Defer { body, .. } => self.visit_expr(body),
            Stmt::ConsumeScope { init, body, .. } => {
                self.visit_expr(init);
                self.visit_block(body);
            }
            Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => self.visit_expr(expr),
            Stmt::Apply { args, .. } => {
                for a in args {
                    self.visit_expr(a);
                }
            }
            Stmt::Calc { steps, .. } => {
                for s in steps {
                    self.visit_expr(&s.expr);
                }
            }
            _ => {}
        }
    }

    fn visit_expr(&mut self, expr: &Expr) {
        match &expr.kind {
            ExprKind::Call { func, args, .. } => {
                self.emit_param_hints(func, args);
                self.visit_expr(func);
                for a in args {
                    self.visit_expr(a.expr());
                }
            }
            ExprKind::For { iter, body, .. } | ExprKind::ParallelFor { iter, body, .. } => {
                self.visit_expr(iter);
                self.visit_block(body);
            }
            ExprKind::IfLet { scrutinee, guard, then, else_, .. } => {
                self.visit_expr(scrutinee);
                if let Some(g) = guard {
                    self.visit_expr(g);
                }
                self.visit_block(then);
                self.visit_else(else_);
            }
            ExprKind::WhileLet { scrutinee, guard, body, .. } => {
                self.visit_expr(scrutinee);
                if let Some(g) = guard {
                    self.visit_expr(g);
                }
                self.visit_block(body);
            }
            ExprKind::If { cond, then, else_, .. } => {
                self.visit_expr(cond);
                self.visit_block(then);
                self.visit_else(else_);
            }
            ExprKind::While { cond, body, .. } => {
                self.visit_expr(cond);
                self.visit_block(body);
            }
            ExprKind::Loop { body, .. } => self.visit_block(body),
            ExprKind::Block(b) => self.visit_block(b),
            ExprKind::Match { scrutinee, arms, .. } => {
                self.visit_expr(scrutinee);
                for arm in arms {
                    if let Some(g) = &arm.guard {
                        self.visit_expr(g);
                    }
                    match &arm.body {
                        MatchArmBody::Expr(e) => self.visit_expr(e),
                        MatchArmBody::Block(b) => self.visit_block(b),
                    }
                }
            }
            ExprKind::ClosureLight { body, .. } => match body {
                ClosureBody::Expr(e) => self.visit_expr(e),
                ClosureBody::Block(b) => self.visit_block(b),
            },
            ExprKind::ClosureFull(sig_body) => match &sig_body.body {
                FnBody::Block(b) => self.visit_block(b),
                FnBody::Expr(e) => self.visit_expr(e),
                FnBody::External => {}
            },
            ExprKind::Member { obj, .. } => self.visit_expr(obj),
            ExprKind::Index { obj, index } => {
                self.visit_expr(obj);
                self.visit_expr(index);
            }
            ExprKind::Binary { left, right, .. } => {
                self.visit_expr(left);
                self.visit_expr(right);
            }
            ExprKind::Unary { operand, .. } => self.visit_expr(operand),
            ExprKind::TupleLit(elems) => {
                for e in elems {
                    self.visit_expr(e);
                }
            }
            ExprKind::ArrayLit(elems) => {
                for e in elems {
                    match e {
                        ArrayElem::Item(x) | ArrayElem::Spread(x) => self.visit_expr(x),
                    }
                }
            }
            ExprKind::RecordLit { fields, .. } => {
                for f in fields {
                    if let Some(v) = &f.value {
                        self.visit_expr(v);
                    }
                }
            }
            ExprKind::TurboFish { base, .. } => self.visit_expr(base),
            ExprKind::Forbid { body, .. } | ExprKind::Blocking(body) => self.visit_block(body),
            ExprKind::Supervised { body, cancel, .. } => {
                self.visit_block(body);
                if let Some(c) = cancel {
                    self.visit_expr(c);
                }
            }
            ExprKind::Try(inner)
            | ExprKind::Bang(inner)
            | ExprKind::Spawn(inner)
            | ExprKind::Throw(inner) => self.visit_expr(inner),
            ExprKind::As(inner, _) | ExprKind::Is(inner, _) => self.visit_expr(inner),
            ExprKind::Coalesce(a, b) => {
                self.visit_expr(a);
                self.visit_expr(b);
            }
            ExprKind::Range { start, end, .. } => {
                if let Some(e) = start {
                    self.visit_expr(e);
                }
                if let Some(e) = end {
                    self.visit_expr(e);
                }
            }
            ExprKind::Forall { range, body, .. } | ExprKind::Exists { range, body, .. } => {
                self.visit_expr(range);
                self.visit_expr(body);
            }
            _ => {}
        }
    }

    fn visit_else(&mut self, else_: &Option<ElseBranch>) {
        match else_ {
            Some(ElseBranch::Block(b)) => self.visit_block(b),
            Some(ElseBranch::If(e)) => self.visit_expr(e),
            None => {}
        }
    }

    // ── Type hints ────────────────────────────────────────────────────────────

    /// Emit a `: T` type hint for an un-annotated simple binding `ro x = expr`.
    ///
    /// No hint when: type hints are disabled; the binding already has an explicit
    /// type; the pattern is not a single identifier (tuple/record destructuring —
    /// a single trailing `: T` would be wrong); the initializer's type is unknown
    /// (absent from `expr_types` — the IDE degrades to nothing rather than guess).
    fn emit_type_hint(&mut self, ld: &LetDecl) {
        if !self.cfg.type_hints {
            return;
        }
        if ld.ty.is_some() {
            return;
        }
        let Pattern::Ident { name, span, .. } = &ld.pattern else {
            return;
        };
        // Only project spans belonging to the current buffer.
        if span.file_id != MAIN_FILE_ID {
            return;
        }
        let Some(env) = self.env else {
            return;
        };
        let Some(tr) = env.expr_types.get(&ld.value.span) else {
            return;
        };
        let end = ident_end(self.src, *span, name);
        let pos = byte_offset_to_position(self.rope, end);
        if !in_range(pos, self.range) {
            return;
        }
        // Nova type syntax is space-separated (`consume s TcpStream`), not
        // colon-separated — the hint must read as insertable Nova code
        // (owner 2026-07-23; was Rust/TS-style ": T").
        //
        // The binding keyword already carries the qualifier, so it must not be
        // repeated in the hint: `ro nk = branch_children(...)` where the callee
        // returns `ro []Node` used to render as `ro nk ro []Node`, which is not
        // Nova and does not read as anything (owner report 2026-08-17 on
        // `novac/src/sem/sem.nv`, registry №709). Peel top-level `ro`/`mut`
        // wrappers — and only those: `consume` is part of the type's identity
        // (D131), not a binding mode, and a nested `ro` inside a generic argument
        // (`Vec[ro Node]`) is genuine type information the reader needs.
        let mut shown = tr;
        loop {
            match shown {
                TypeRef::Readonly(inner, _) | TypeRef::Mut(inner, _) => shown = inner,
                _ => break,
            }
        }
        let label = format!(" {}", format_type_ref(shown));
        self.out.push(mk_hint(pos, label, InlayHintKind::TYPE, false, false));
    }

    // ── Parameter-name hints ───────────────────────────────────────────────────

    /// Emit `name:` hints before each positional argument of a call.
    ///
    /// The callee (free function or value method) is resolved from the module AST
    /// by name + arity; when the overload set cannot be disambiguated the call is
    /// skipped entirely (no wrong hints). Named arguments already show the name, so
    /// they are ignored for counting *and* rendering; spread `...xs` gets no hint.
    fn emit_param_hints(&mut self, func: &Expr, args: &[CallArg]) {
        if !self.cfg.parameter_hints {
            return;
        }
        let func = func.unwrap_turbofish();
        let (name, is_method) = match &func.kind {
            ExprKind::Ident(n) => (n.clone(), false),
            ExprKind::Path(segs) => match segs.last() {
                Some(s) => (s.clone(), false),
                None => return,
            },
            ExprKind::Member { name, .. } => (name.clone(), true),
            _ => return,
        };

        let Some(params) = self.resolve_params(&name, is_method, args) else {
            return;
        };

        // Walk positional (`Item`) arguments only, in source order, mapping each to
        // the parameter at the same index.
        let mut idx = 0usize;
        for a in args {
            let arg = match a {
                CallArg::Item(e) => e,
                // Named args already display their name; spreads have no single slot.
                CallArg::Named { .. } | CallArg::Spread(_) => continue,
            };
            let Some(p) = params.get(idx) else {
                break;
            };
            idx += 1;
            // Do not annotate the variadic tail — one `xs:` before the first of
            // many packed arguments would be misleading.
            if p.is_variadic {
                break;
            }
            // Redundant hint suppression: `f(count)` where the parameter is `count`.
            if let ExprKind::Ident(a_name) = &arg.unwrap_turbofish().kind {
                if *a_name == p.name {
                    continue;
                }
            }
            if arg.span.file_id != MAIN_FILE_ID {
                continue;
            }
            let pos = byte_offset_to_position(self.rope, arg.span.start);
            if !in_range(pos, self.range) {
                continue;
            }
            let label = format!("{}:", p.name);
            self.out.push(mk_hint(pos, label, InlayHintKind::PARAMETER, false, true));
        }
    }

    /// Resolve the parameter list of the called overload, or `None` when it cannot
    /// be pinned down unambiguously.
    ///
    /// Candidate ordering follows the call kind (method call prefers methods, free
    /// call prefers free functions), matching signature-help. Selection:
    /// 1. exactly one overload whose arity equals the positional-arg count → it;
    /// 2. otherwise, exactly one candidate total → it (tolerates a partially-typed
    ///    call whose arity does not yet match);
    /// 3. otherwise, exactly one *variadic* candidate that can absorb the args → it;
    /// 4. otherwise → `None` (ambiguous; emit nothing rather than a wrong name).
    fn resolve_params(
        &self,
        name: &str,
        is_method: bool,
        args: &[CallArg],
    ) -> Option<Vec<ParamInfo>> {
        let free = find_fn_by_name(self.module, name);
        let methods = find_method_by_name(self.module, name);
        let candidates: Vec<&FnDecl> = if is_method {
            methods.into_iter().chain(free).collect()
        } else {
            free.into_iter().chain(methods).collect()
        };
        if candidates.is_empty() {
            return None;
        }

        let argc = args
            .iter()
            .filter(|a| !matches!(a, CallArg::Named { .. }))
            .count();

        let exact: Vec<&FnDecl> = candidates
            .iter()
            .copied()
            .filter(|fd| fd.params.len() == argc)
            .collect();

        let chosen: &FnDecl = if exact.len() == 1 {
            exact[0]
        } else if candidates.len() == 1 {
            candidates[0]
        } else {
            let variadic: Vec<&FnDecl> = candidates
                .iter()
                .copied()
                .filter(|fd| {
                    fd.params.last().map_or(false, |p| p.is_variadic)
                        && argc + 1 >= fd.params.len()
                })
                .collect();
            if variadic.len() == 1 {
                variadic[0]
            } else {
                return None;
            }
        };

        Some(
            chosen
                .params
                .iter()
                .map(|p| ParamInfo { name: p.name.clone(), is_variadic: p.is_variadic })
                .collect(),
        )
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Byte offset just past the identifier `name` inside `span` (where the `: T`
/// hint anchors). Falls back to `span.end` if the name cannot be located (e.g. a
/// `mut`-prefixed pattern span wider than the identifier).
fn ident_end(src: &str, span: Span, name: &str) -> usize {
    let lo = span.start.min(src.len());
    let hi = span.end.min(src.len());
    if lo <= hi && src.is_char_boundary(lo) && src.is_char_boundary(hi) {
        if let Some(off) = src[lo..hi].find(name) {
            return lo + off + name.len();
        }
    }
    hi
}

/// Build an `InlayHint` at `pos` with a plain-string `label`.
fn mk_hint(
    pos: Position,
    label: String,
    kind: InlayHintKind,
    padding_left: bool,
    padding_right: bool,
) -> InlayHint {
    InlayHint {
        position: pos,
        label: InlayHintLabel::String(label),
        kind: Some(kind),
        text_edits: None,
        tooltip: None,
        padding_left: Some(padding_left),
        padding_right: Some(padding_right),
        data: None,
    }
}

/// True when `p` lies within `[range.start, range.end]` (inclusive) in the LSP
/// UTF-16 coordinate space.
fn in_range(p: Position, range: Range) -> bool {
    !(pos_lt(p, range.start) || pos_lt(range.end, p))
}

fn pos_lt(a: Position, b: Position) -> bool {
    a.line < b.line || (a.line == b.line && a.character < b.character)
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provenance::resolve_module_for_ide;
    use std::path::PathBuf;

    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("nova-lsp has a parent")
            .to_path_buf()
    }

    fn write_temp(stem: &str, src: &str) -> PathBuf {
        let dir = repo_root().join("target").join("inlay_test").join(stem);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("{stem}.nv"));
        std::fs::write(&path, src).unwrap();
        path
    }

    /// Whole-document range (covers every line).
    fn full_range() -> Range {
        Range {
            start: Position { line: 0, character: 0 },
            end: Position { line: u32::MAX, character: 0 },
        }
    }

    fn hints(stem: &str, src: &str, cfg: InlayHintConfig) -> Vec<InlayHint> {
        let path = write_temp(stem, src);
        let resolved = resolve_module_for_ide(&path, src);
        compute_inlay_hints_in(&resolved, src, full_range(), cfg)
    }

    fn label_str(h: &InlayHint) -> String {
        match &h.label {
            InlayHintLabel::String(s) => s.clone(),
            InlayHintLabel::LabelParts(parts) => {
                parts.iter().map(|p| p.value.clone()).collect()
            }
        }
    }

    // ── POS: type hint ─────────────────────────────────────────────────────────

    #[test]
    fn pos_type_hint_int() {
        let src = concat!(
            "module basics.lsp\n",
            "fn main() -> () {\n",
            "    ro x = 5\n",
            "}\n",
        );
        let hs = hints("pos_type_int", src, InlayHintConfig::default());
        let type_hints: Vec<_> = hs
            .iter()
            .filter(|h| h.kind == Some(InlayHintKind::TYPE))
            .collect();
        assert_eq!(type_hints.len(), 1, "one un-annotated binding → one type hint");
        assert_eq!(label_str(type_hints[0]), " int", "`ro x = 5` → ` int`");
        // Anchored right after `x` on line 2 (0-based).
        assert_eq!(type_hints[0].position.line, 2);
    }

    // ── POS: parameter-name hint ───────────────────────────────────────────────

    #[test]
    fn pos_param_hint_name() {
        let src = concat!(
            "module basics.lsp\n",
            "fn add(a int, b int) -> int => a + b\n",
            "fn main() -> () {\n",
            "    ro _ = add(1, 2)\n",
            "}\n",
        );
        let hs = hints("pos_param_name", src, InlayHintConfig::default());
        let params: Vec<String> = hs
            .iter()
            .filter(|h| h.kind == Some(InlayHintKind::PARAMETER))
            .map(label_str)
            .collect();
        assert_eq!(params, vec!["a:".to_string(), "b:".to_string()], "both param names shown");
    }

    #[test]
    fn pos_param_hint_single_arg() {
        // Plan criterion: param-hint on `foo(1)` → `a:`.
        let src = concat!(
            "module basics.lsp\n",
            "fn foo(a int) -> int => a\n",
            "fn main() -> () {\n",
            "    ro _ = foo(1)\n",
            "}\n",
        );
        let hs = hints("pos_param_single", src, InlayHintConfig::default());
        let params: Vec<String> = hs
            .iter()
            .filter(|h| h.kind == Some(InlayHintKind::PARAMETER))
            .map(label_str)
            .collect();
        assert_eq!(params, vec!["a:".to_string()]);
    }

    // ── NEG: annotated binding → no type hint ──────────────────────────────────

    #[test]
    fn neg_annotated_no_type_hint() {
        let src = concat!(
            "module basics.lsp\n",
            "fn main() -> () {\n",
            "    ro x int = 5\n",
            "}\n",
        );
        let hs = hints("neg_annotated", src, InlayHintConfig::default());
        assert!(
            hs.iter().all(|h| h.kind != Some(InlayHintKind::TYPE)),
            "an explicitly-annotated binding gets no type hint"
        );
    }

    // ── NEG: redundant param hint suppressed ───────────────────────────────────

    #[test]
    fn neg_redundant_param_hint_suppressed() {
        let src = concat!(
            "module basics.lsp\n",
            "fn use_count(count int) -> int => count\n",
            "fn main() -> () {\n",
            "    ro count = 3\n",
            "    ro _ = use_count(count)\n",
            "}\n",
        );
        let hs = hints("neg_redundant", src, InlayHintConfig::default());
        let params: Vec<String> = hs
            .iter()
            .filter(|h| h.kind == Some(InlayHintKind::PARAMETER))
            .map(label_str)
            .collect();
        assert!(
            params.is_empty(),
            "`use_count(count)` where param is `count` → no redundant hint, got {params:?}"
        );
    }

    // ── Config: off-by-config ──────────────────────────────────────────────────

    /// A binding with a literal value (`ro n = 5` → `: int` from syntactic
    /// inference) plus a call (`add(1, 2)` → `a:`/`b:`) so each toggle can be
    /// verified against a hint kind that is actually produced.
    const CFG_SRC: &str = concat!(
        "module basics.lsp\n",
        "fn add(a int, b int) -> int => a + b\n",
        "fn main() -> () {\n",
        "    ro n = 5\n",
        "    ro _ = add(1, 2)\n",
        "}\n",
    );

    #[test]
    fn config_type_hints_off() {
        let cfg = InlayHintConfig { type_hints: false, parameter_hints: true };
        let hs = hints("cfg_type_off", CFG_SRC, cfg);
        assert!(hs.iter().all(|h| h.kind != Some(InlayHintKind::TYPE)), "type hints disabled");
        assert!(hs.iter().any(|h| h.kind == Some(InlayHintKind::PARAMETER)), "param hints still on");
    }

    #[test]
    fn config_param_hints_off() {
        let cfg = InlayHintConfig { type_hints: true, parameter_hints: false };
        let hs = hints("cfg_param_off", CFG_SRC, cfg);
        assert!(hs.iter().all(|h| h.kind != Some(InlayHintKind::PARAMETER)), "param hints disabled");
        assert!(hs.iter().any(|h| h.kind == Some(InlayHintKind::TYPE)), "type hints still on");
    }

    #[test]
    fn config_from_settings_parsing() {
        let v = serde_json::json!({
            "nova": { "inlayHints": { "typeHints": false, "parameterHints": true } }
        });
        let cfg = InlayHintConfig::from_settings(&v);
        assert!(!cfg.type_hints);
        assert!(cfg.parameter_hints);

        // Master switch disables both.
        let v2 = serde_json::json!({ "nova": { "inlayHints": { "enable": false } } });
        let cfg2 = InlayHintConfig::from_settings(&v2);
        assert!(!cfg2.type_hints && !cfg2.parameter_hints);

        // Empty settings → defaults (both on).
        let cfg3 = InlayHintConfig::from_settings(&serde_json::json!({}));
        assert_eq!(cfg3, InlayHintConfig::default());
    }

    // ── EDGE: multi-byte (UTF-16) positions ────────────────────────────────────

    #[test]
    fn edge_multibyte_param_hint_utf16_column() {
        // A multi-byte character (`é`, 2 UTF-8 bytes / 1 UTF-16 unit) inside the
        // first argument shifts the byte column of the second argument by +1 over
        // its UTF-16 column. The `count:` hint before `2` must use the UTF-16
        // column; a byte-based (buggy) computation would be one greater.
        let src = concat!(
            "module basics.lsp\n",
            "fn greet(msg str, count int) => ()\n",
            "fn main() -> () {\n",
            "    ro _ = greet(\"h\u{00E9}llo\", 2)\n",
            "}\n",
        );
        let hs = hints("edge_mb_utf16", src, InlayHintConfig::default());
        let count_hint = hs
            .iter()
            .find(|h| h.kind == Some(InlayHintKind::PARAMETER) && label_str(h) == "count:")
            .expect("count: hint present");
        assert_eq!(count_hint.position.line, 3);

        // Independent cross-check: locate `2`'s byte column on its line. Exactly one
        // multi-byte char (`é`) precedes it, so UTF-16 column == byte column − 1.
        let line = "    ro _ = greet(\"h\u{00E9}llo\", 2)";
        let byte_col_of_2 = line.rfind('2').expect("has `2`");
        assert_eq!(
            count_hint.position.character as usize + 1,
            byte_col_of_2,
            "hint uses UTF-16 column, not byte offset"
        );
    }

    #[test]
    fn edge_cyrillic_comments_above_do_not_shift_param_hints() {
        // Registry #709, reported by the owner 2026-08-17 on
        // `novac/src/parse/parse.nv`: in a file whose comments are Cyrillic,
        // parameter hints landed INSIDE words (`Recovery ins` + `v: ` + `ide`).
        // Cyrillic is 2 UTF-8 bytes per 1 UTF-16 unit, so any stage that treats
        // a byte offset as a char index -- or a char index as a byte offset --
        // drifts by the count of such characters BEFORE the hint. The comments
        // here sit on lines the hints are not on, which is exactly the reported
        // shape: the drift must not accumulate across preceding lines.
        let src = concat!(
            "module basics.lsp\n",
            "// \u{0412}\u{043E}\u{0441}\u{0441}\u{0442}\u{0430}\u{043D}\u{043E}\u{0432}\u{043B}\u{0435}\u{043D}\u{0438}\u{0435} \u{043F}\u{043E}\u{0441}\u{043B}\u{0435} \u{043E}\u{0448}\u{0438}\u{0431}\u{043A}\u{0438} \u{0440}\u{0430}\u{0437}\u{0431}\u{043E}\u{0440}\u{0430}: \u{043F}\u{0440}\u{043E}\u{043F}\u{0443}\u{0441}\u{043A}\u{0430}\u{0435}\u{043C} \u{0434}\u{043E} \u{0442}\u{043E}\u{0447}\u{043A}\u{0438} \u{0441}\u{0438}\u{043D}\u{0445}\u{0440}\u{043E}\u{043D}\u{0438}\u{0437}\u{0430}\u{0446}\u{0438}\u{0438}.\n",
            "// \u{041F}\u{0440}\u{0430}\u{0432}\u{0438}\u{043B}\u{043E} \u{0437}\u{0430}\u{043F}\u{0438}\u{0441}\u{0430}\u{043D}\u{043E} \u{0432} D-\u{0431}\u{043B}\u{043E}\u{043A}\u{0435}; \u{0437}\u{0434}\u{0435}\u{0441}\u{044C} \u{0442}\u{043E}\u{043B}\u{044C}\u{043A}\u{043E} \u{0440}\u{0435}\u{0430}\u{043B}\u{0438}\u{0437}\u{0430}\u{0446}\u{0438}\u{044F} \u{043E}\u{0431}\u{0445}\u{043E}\u{0434}\u{0430}.\n",
            "fn add(a int, b int) -> int => a + b\n",
            "fn main() -> () {\n",
            "    ro _ = add(1, 2)\n",
            "}\n",
        );
        let hs = hints("edge_cyr_comments", src, InlayHintConfig::default());
        let params: Vec<_> = hs
            .iter()
            .filter(|h| h.kind == Some(InlayHintKind::PARAMETER))
            .collect();
        assert_eq!(params.len(), 2, "both param hints present");

        // The call is on line 5 (0-based). Columns are the columns of `1` and `2`
        // on that line -- the line is pure ASCII, so UTF-16 column == byte column.
        let line = "    ro _ = add(1, 2)";
        let col1 = line.find('1').expect("has `1`");
        let col2 = line.rfind('2').expect("has `2`");
        assert_eq!(
            (params[0].position.line, params[0].position.character as usize),
            (5, col1),
            "`a:` anchors at the first argument, not shifted by Cyrillic above"
        );
        assert_eq!(
            (params[1].position.line, params[1].position.character as usize),
            (5, col2),
            "`b:` anchors at the second argument, not shifted by Cyrillic above"
        );
    }

    #[test]
    fn edge_ro_return_type_hint_drops_binding_qualifier() {
        // Registry №709 carrier, reported by the owner 2026-08-17 on
        // `novac/src/sem/sem.nv`: the callee returns `ro []Node`, the binding is
        // already `ro`, and the hint rendered the qualifier a second time --
        // `ro nk ro []Node`. The hint must stay insertable after `ro nk`.
        let src = concat!(
            "module basics.lsp\n",
            "type Node { id int }\n",
            "fn branch_children() -> ro []Node => []Node.new()\n",
            "fn main() -> () {\n",
            "    ro nk = branch_children()\n",
            "}\n",
        );
        let hs = hints("edge_ro_hint", src, InlayHintConfig::default());
        let tys: Vec<String> = hs
            .iter()
            .filter(|h| h.kind == Some(InlayHintKind::TYPE))
            .map(label_str)
            .collect();
        assert_eq!(
            tys,
            vec![" []Node".to_string()],
            "the binding's `ro` must not be repeated by the hint"
        );
    }

    // ── EDGE: range filtering ──────────────────────────────────────────────────

    #[test]
    fn edge_range_filters_out_of_view_hints() {
        let src = concat!(
            "module basics.lsp\n",
            "fn main() -> () {\n",
            "    ro a = 1\n",
            "    ro b = 2\n",
            "    ro c = 3\n",
            "}\n",
        );
        let path = write_temp("edge_range", src);
        let resolved = resolve_module_for_ide(&path, src);
        // Only line 3 (`ro b = 2`) is in range.
        let range = Range {
            start: Position { line: 3, character: 0 },
            end: Position { line: 3, character: 20 },
        };
        let hs = compute_inlay_hints_in(&resolved, src, range, InlayHintConfig::default());
        let type_hints: Vec<_> = hs
            .iter()
            .filter(|h| h.kind == Some(InlayHintKind::TYPE))
            .collect();
        assert_eq!(type_hints.len(), 1, "only the in-range binding yields a hint");
        assert_eq!(type_hints[0].position.line, 3);
    }

    // ── EDGE: parse error degrades to no hints (no panic) ──────────────────────

    #[test]
    fn edge_parse_error_no_panic() {
        let src = "module basics.lsp\nfn broken(@@@@ =>";
        let hs = hints("edge_parse_err", src, InlayHintConfig::default());
        assert!(hs.is_empty(), "parse error → no hints, no panic");
    }
}
