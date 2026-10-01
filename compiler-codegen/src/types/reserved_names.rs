//! D487 (owner decision 2026-10-01, registry 221.1 #1440 question 1): a name
//! the PROGRAMMER declares may not begin with one of the four namespaces the
//! compiler keeps for its own names in the generated C -- `_nv_`, `_nova_`,
//! `_at_`, `__`.
//!
//! Why: the one door "Nova name -> C identifier" (`codegen/emit_c/c_name.rs`,
//! `c_ident`) escapes every risky name to `nv_` + name, EXCEPT these four
//! namespaces, which pass unchanged because the parser, the desugar passes
//! and the generator synthesize names there (`_nv_tmp_N`, `_nova_decr_old`,
//! `_at_<F>`, `__nova_arg_srcN`) and read them back through the same `Ident`
//! path as user names. A user name in the same space would reach C raw and
//! could meet one of those: the last hole in the door's injectivity. The
//! door's exemption list IS this module's list ([`COMPILER_NAMESPACES`]), so
//! the two can never drift apart.
//!
//! Which declarations: every name the programmer binds -- locals
//! (`ro`/`mut`/`consume`, `consume ... as`, its result name, `defer(o)`),
//! parameters (functions, methods, effect and protocol operations, lambdas,
//! light and full closures, trailing blocks, handler-literal methods),
//! record and named-tuple fields, sum variants and their record fields,
//! functions, methods, types, constants (module, local, associated),
//! module-level values, effects and their operations, protocols and their
//! methods, generic parameters, lemmas, axioms and their binders, quantifier
//! variables, bench parameters, `select` receive bindings and every name a
//! pattern binds (identifiers, `name @ pat`, `..rest`, record shorthand).
//! NOT judged: the name of an `extern "C"` function -- it is the literal C
//! symbol of a foreign library (D282), the compiler neither chooses nor
//! renames it. The language has no loop labels, so there is nothing to judge
//! there.
//!
//! Where it runs, and why there: the rule is about what the programmer
//! WROTE, and only the parser can tell a written name from a synthesized one.
//! `check_module` sees the tree after `auto_derive` (serde, pre-check) and
//! runs again after `callnorm` / `field_cache` (`test_runner`'s
//! post-normalize re-check, whose error SILENTLY degrades the annotation
//! channel) -- by then `__nova_recv`, `_at_x`, `__nv_hf_*` are ordinary
//! `Ident`s, and the AST carries no name spans to tell them apart. So the
//! parser calls [`check_parsed_module`] once, on the tree it has just built,
//! with its own token stream: a declaration counts only when an identifier
//! TOKEN spelling its name lies inside the declaration's span (the two names
//! the parser itself synthesizes -- `_nova_decr_old` for `decreases`,
//! `__embed_<T>` for an anonymous embed -- have no such token). The walk
//! starts only when the token stream holds a reserved-prefix identifier at
//! all, so an ordinary file pays one pass over its tokens.

use crate::ast::*;
use crate::diag::{Diagnostic, Span};
use crate::lexer::{Token, TokenKind};

/// The compiler's own namespaces in the generated C. `c_ident` lets these
/// through unescaped; D487 keeps every programmer-declared name out of them.
pub(crate) const COMPILER_NAMESPACES: &[&str] = &["_nv_", "_nova_", "_at_", "__"];

/// The reserved namespace `name` starts with, if any.
pub(crate) fn reserved_prefix(name: &str) -> Option<&'static str> {
    COMPILER_NAMESPACES.iter().copied().find(|p| name.starts_with(p))
}

/// D487 over one freshly parsed file: the first programmer-declared name in
/// a reserved namespace, as an `E_RESERVED_NAME` diagnostic on the name token.
pub(crate) fn check_parsed_module(items: &[Item], tokens: &[Token], src: &str, src_base: usize) -> Option<Diagnostic> {
    let any_reserved = tokens
        .iter()
        .any(|t| matches!(&t.kind, TokenKind::Ident(s) if reserved_prefix(s).is_some()));
    // `${...}` bodies are lexed by a sub-parser, so their identifiers are not
    // in `tokens`; the source text covers them.
    if !any_reserved && !src_mentions_reserved(src) {
        return None;
    }
    let mut w = Walker { out: Vec::new(), pat_kind: "binding" };
    for it in items {
        w.item(it);
    }
    let mut found: Vec<(Span, &'static str, String)> = Vec::new();
    for (name, span, kind) in w.out {
        if reserved_prefix(&name).is_none() {
            continue;
        }
        if let Some(at) = written_at(&name, span, tokens, src, src_base) {
            found.push((at, kind, name));
        }
    }
    found.sort_by_key(|(s, _, _)| s.start);
    found.into_iter().next().map(|(at, kind, name)| diagnostic(&name, kind, at))
}

fn src_mentions_reserved(src: &str) -> bool {
    src.contains("${") && COMPILER_NAMESPACES.iter().any(|p| src.contains(p))
}

/// The span of the identifier token that spells `name` inside `decl`, or --
/// for a name inside a `${...}` body, which has no token in this stream -- of
/// a whole-identifier occurrence of `name` in the source text of `decl`.
/// `None`: the name was not written there, i.e. the parser synthesized it.
fn written_at(name: &str, decl: Span, tokens: &[Token], src: &str, src_base: usize) -> Option<Span> {
    if decl.end <= decl.start {
        return None;
    }
    let first = tokens.partition_point(|t| t.span.start < decl.start);
    for t in &tokens[first..] {
        if t.span.start >= decl.end {
            break;
        }
        if matches!(&t.kind, TokenKind::Ident(s) if s == name) {
            return Some(t.span);
        }
    }
    // Inside a `${...}` body: the declaration lies within one string token.
    let in_string = tokens[..first]
        .last()
        .map_or(false, |t| t.span.start <= decl.start && decl.end <= t.span.end && !matches!(t.kind, TokenKind::Ident(_)));
    if !in_string {
        return None;
    }
    let (a, b) = (decl.start.checked_sub(src_base)?, decl.end.checked_sub(src_base)?);
    let text = src.get(a..b)?;
    let is_ident = |c: char| c.is_ascii_alphanumeric() || c == '_';
    let mut from = 0;
    while let Some(off) = text[from..].find(name) {
        let s = from + off;
        let e = s + name.len();
        let before_ok = text[..s].chars().next_back().map_or(true, |c| !is_ident(c));
        let after_ok = text[e..].chars().next().map_or(true, |c| !is_ident(c));
        if before_ok && after_ok {
            return Some(Span { start: decl.start + s, end: decl.start + e, file_id: decl.file_id });
        }
        from = s + 1;
    }
    None
}

fn diagnostic(name: &str, kind: &str, at: Span) -> Diagnostic {
    let prefix = reserved_prefix(name).unwrap_or("__");
    let stripped = name.trim_start_matches('_');
    let hint = if stripped.is_empty() || reserved_prefix(stripped).is_some() {
        "a name that does not start with `_nv_`, `_nova_`, `_at_` or `__`".to_string()
    } else {
        format!("`{}`", stripped)
    };
    Diagnostic::new(
        format!(
            "[E_RESERVED_NAME] {kind} `{name}` is declared in the compiler's reserved \
             namespace `{prefix}` (D487): names starting with `_nv_`, `_nova_`, `_at_` \
             or `__` belong to the code generator, which writes its own names there in \
             the generated C, and a program name in the same space could collide with \
             one of them. Rename it, e.g. {hint}. (`_x` and `_` stay lawful: D461.)"
        ),
        at,
    )
}

/// Collects every programmer-bindable declaration as (name, span to search
/// for the name token, kind word for the message).
struct Walker {
    out: Vec<(String, Span, &'static str)>,
    /// What a name bound by a pattern is called in the message.
    pat_kind: &'static str,
}

impl Walker {
    fn decl(&mut self, name: &str, span: Span, kind: &'static str) {
        self.out.push((name.to_string(), span, kind));
    }

    fn generics(&mut self, gs: &[GenericParam]) {
        for g in gs {
            self.decl(&g.name, g.span, "generic parameter");
        }
    }

    fn params(&mut self, ps: &[Param]) {
        for p in ps {
            self.decl(&p.name, p.span, "parameter");
            if let Some(d) = &p.default {
                self.expr(d);
            }
        }
    }

    fn contracts(&mut self, cs: &[Contract]) {
        for c in cs {
            self.expr(&c.expr);
        }
    }

    fn fn_body(&mut self, b: &FnBody) {
        match b {
            FnBody::Expr(x) => self.expr(x),
            FnBody::Block(b) => self.block(b),
            FnBody::External => {}
        }
    }

    fn item(&mut self, it: &Item) {
        match it {
            Item::Fn(f) => self.fn_decl(f),
            Item::Type(t) => self.type_decl(t),
            Item::Let(d) => {
                self.pat_kind = "module-level value";
                self.pattern(&d.pattern);
                self.pat_kind = "binding";
                self.expr(&d.value);
            }
            Item::Const(c) => {
                self.decl(&c.name, c.span, "constant");
                self.expr(&c.value);
            }
            Item::Test(t) => self.block(&t.body),
            Item::Bench(b) => {
                if let Some(p) = &b.params {
                    self.decl(&p.var_name, p.span, "bench parameter");
                }
                self.stmts(&b.setup);
                self.block(&b.measure_body);
                self.stmts(&b.teardown);
                for g in &b.groups {
                    for c in &g.cases {
                        self.stmts(&c.setup);
                        self.block(&c.measure_body);
                        self.stmts(&c.teardown);
                    }
                }
            }
            Item::Lemma(l) => {
                self.decl(&l.name, l.span, "lemma");
                self.generics(&l.generics);
                self.params(&l.params);
                self.contracts(&l.contracts);
                self.fn_body(&l.body);
            }
        }
    }

    fn fn_decl(&mut self, f: &FnDecl) {
        // D282: an `extern "C"` name is the literal symbol of a C library.
        if f.extern_abi.as_deref() != Some("C") && !f.compiler_generated {
            let kind = if f.receiver.is_some() { "method" } else { "function" };
            self.decl(&f.name, f.span, kind);
        }
        if f.compiler_generated {
            return;
        }
        if let Some(r) = &f.receiver {
            self.generics(&r.carrier_bounds);
        }
        self.generics(&f.generics);
        self.params(&f.params);
        self.contracts(&f.contracts);
        if let Some(d) = &f.decreases {
            self.expr(d);
        }
        self.fn_body(&f.body);
    }

    fn type_decl(&mut self, t: &TypeDecl) {
        let kind = match &t.kind {
            TypeDeclKind::Effect(_) => "effect",
            TypeDeclKind::Protocol { .. } => "protocol",
            _ => "type",
        };
        self.decl(&t.name, t.span, kind);
        self.generics(&t.generics);
        match &t.kind {
            TypeDeclKind::Record(fields) => self.fields(fields),
            TypeDeclKind::Sum(vs) => {
                for v in vs {
                    self.decl(&v.name, v.span, "variant");
                    if let SumVariantKind::Record(fields) = &v.kind {
                        self.fields(fields);
                    }
                }
            }
            TypeDeclKind::Effect(ms) | TypeDeclKind::Protocol { methods: ms, .. } => {
                let op = if matches!(t.kind, TypeDeclKind::Effect(_)) { "effect operation" } else { "protocol method" };
                for m in ms {
                    self.decl(&m.name, m.span, op);
                    self.generics(&m.generics);
                    self.params(&m.params);
                    self.contracts(&m.contracts);
                    if let Some(b) = &m.default_body {
                        self.block(b);
                    }
                }
            }
            TypeDeclKind::NamedTuple(fs) => {
                for f in fs {
                    self.decl(&f.name, f.span, "field");
                    if let Some(d) = &f.default {
                        self.expr(d);
                    }
                }
            }
            TypeDeclKind::Newtype(_) | TypeDeclKind::Alias(_) | TypeDeclKind::TypeSet(_) | TypeDeclKind::Opaque => {}
        }
        for c in &t.assoc_consts {
            self.decl(&c.name, c.span, "constant");
            self.expr(&c.value);
        }
        self.contracts(&t.invariants);
        for a in &t.axioms {
            self.decl(&a.name, a.span, "axiom");
            self.generics(&a.generics);
            for b in &a.binders {
                self.decl(&b.name, b.span, "axiom binder");
            }
            self.expr(&a.formula);
        }
    }

    fn fields(&mut self, fields: &[RecordField]) {
        for f in fields {
            self.decl(&f.name, f.span, "field");
        }
    }

    fn let_decl(&mut self, d: &LetDecl) {
        self.pattern(&d.pattern);
        self.expr(&d.value);
    }

    fn pattern(&mut self, p: &Pattern) {
        match p {
            Pattern::Ident { name, span, .. } => self.decl(name, *span, self.pat_kind),
            Pattern::Binding { name, inner, span } => {
                self.decl(name, *span, self.pat_kind);
                self.pattern(inner);
            }
            Pattern::Tuple(ps, _) => {
                for p in ps {
                    self.pattern(p);
                }
            }
            Pattern::Array { elems, span } => {
                for el in elems {
                    match el {
                        ArrayPatternElem::Item(p) => self.pattern(p),
                        ArrayPatternElem::RestBind(name) => self.decl(name, *span, self.pat_kind),
                        ArrayPatternElem::Rest => {}
                    }
                }
            }
            Pattern::Record { fields, .. } => {
                for f in fields {
                    match &f.pattern {
                        Some(p) => self.pattern(p),
                        None => self.decl(&f.name, f.span, self.pat_kind),
                    }
                }
            }
            Pattern::Variant { kind, .. } => {
                if let VariantPatternKind::Tuple { patterns, .. } = kind {
                    for p in patterns {
                        self.pattern(p);
                    }
                }
            }
            Pattern::Or { alternatives, .. } => {
                for p in alternatives {
                    self.pattern(p);
                }
            }
            Pattern::Wildcard(_) | Pattern::Literal(_, _) => {}
        }
    }

    fn block(&mut self, b: &Block) {
        self.stmts(&b.stmts);
        if let Some(t) = &b.trailing {
            self.expr(t);
        }
    }

    fn stmts(&mut self, ss: &[Stmt]) {
        for s in ss {
            self.stmt(s);
        }
    }

    fn stmt(&mut self, s: &Stmt) {
        match s {
            Stmt::Let(d) => self.let_decl(d),
            Stmt::Const(c) => {
                self.decl(&c.name, c.span, "constant");
                self.expr(&c.value);
            }
            Stmt::Expr(e) => self.expr(e),
            Stmt::Assign { target, value, .. } => {
                self.expr(target);
                self.expr(value);
            }
            Stmt::TupleAssign { lhs, rhs, .. } => {
                for e in lhs.iter().chain(rhs) {
                    self.expr(e);
                }
            }
            Stmt::Return { value, .. } => {
                if let Some(v) = value {
                    self.expr(v);
                }
            }
            Stmt::Throw { value, .. } => self.expr(value),
            Stmt::Break(_) | Stmt::Continue(_) | Stmt::Reveal { .. } => {}
            Stmt::Defer { body, outcome_binding, span } => {
                if let Some(o) = outcome_binding {
                    self.decl(o, *span, "binding");
                }
                self.expr(body);
            }
            Stmt::ConsumeScope { binding, init, body, result, span, .. } => {
                self.decl(binding, *span, "binding");
                if let Some(r) = result {
                    self.decl(&r.name, r.span, "binding");
                }
                self.expr(init);
                self.block(body);
            }
            Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => self.expr(expr),
            Stmt::Apply { args, .. } => {
                for a in args {
                    self.expr(a);
                }
            }
            Stmt::Calc { steps, .. } => {
                for st in steps {
                    self.expr(&st.expr);
                }
            }
        }
    }

    fn else_branch(&mut self, eb: &ElseBranch) {
        match eb {
            ElseBranch::Block(b) => self.block(b),
            ElseBranch::If(x) => self.expr(x),
        }
    }

    fn trailing(&mut self, t: &Trailing) {
        match t {
            Trailing::Block(b) => self.block(b),
            Trailing::LegacyBlockWithParams(tb) => {
                for p in &tb.params {
                    self.decl(&p.name, p.span, "parameter");
                }
                self.block(&tb.body);
            }
            Trailing::Fn(sb) => {
                self.params(&sb.params);
                self.fn_body(&sb.body);
            }
        }
    }

    fn expr(&mut self, e: &Expr) {
        match &e.kind {
            ExprKind::Ident(_)
            | ExprKind::Path(_)
            | ExprKind::SelfAccess
            | ExprKind::IntLit(_)
            | ExprKind::FloatLit(_)
            | ExprKind::BoolLit(_)
            | ExprKind::StrLit(_)
            | ExprKind::CharLit(_)
            | ExprKind::UnitLit
            | ExprKind::HexBlobLit(_)
            | ExprKind::NullPtrLit => {}
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
                    if let Some(v) = &f.value {
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
                    self.expr(a.expr());
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
                self.pattern(pattern);
                self.expr(scrutinee);
                if let Some(g) = guard {
                    self.expr(g);
                }
                self.block(then);
                if let Some(eb) = else_ {
                    self.else_branch(eb);
                }
            }
            ExprKind::Match { scrutinee, arms } => {
                self.expr(scrutinee);
                for arm in arms {
                    self.pattern(&arm.pattern);
                    if let Some(g) = &arm.guard {
                        self.expr(g);
                    }
                    match &arm.body {
                        MatchArmBody::Expr(x) => self.expr(x),
                        MatchArmBody::Block(b) => self.block(b),
                    }
                }
            }
            ExprKind::For { pattern, iter, body, invariants, decreases, .. } => {
                self.pattern(pattern);
                self.expr(iter);
                for inv in invariants {
                    self.expr(inv);
                }
                if let Some(d) = decreases {
                    self.expr(d);
                }
                self.block(body);
            }
            ExprKind::ParallelFor { pattern, iter, body, .. } => {
                self.pattern(pattern);
                self.expr(iter);
                self.block(body);
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
                self.pattern(pattern);
                self.expr(scrutinee);
                if let Some(g) = guard {
                    self.expr(g);
                }
                for inv in invariants {
                    self.expr(inv);
                }
                if let Some(d) = decreases {
                    self.expr(d);
                }
                self.block(body);
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
            ExprKind::Supervised { body, cancel, deadline, on_timeout } => {
                if let Some(c) = cancel {
                    self.expr(c);
                }
                if let Some(dl) = deadline {
                    self.expr(&dl.expr);
                }
                if let Some(oh) = on_timeout {
                    self.expr(oh);
                }
                self.block(body);
            }
            ExprKind::Forbid { body, .. } | ExprKind::Realtime { body, .. } => self.block(body),
            ExprKind::Throw(x) => self.expr(x),
            ExprKind::CoalesceReturnFallback(opt) | ExprKind::Interrupt(opt) => {
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
            ExprKind::Lambda { params, body, .. } => {
                for p in params {
                    self.decl(&p.name, p.span, "parameter");
                }
                self.expr(body);
            }
            ExprKind::ClosureLight { params, body } => {
                for p in params {
                    self.decl(&p.name, p.span, "parameter");
                }
                match body {
                    ClosureBody::Expr(x) => self.expr(x),
                    ClosureBody::Block(b) => self.block(b),
                }
            }
            ExprKind::ClosureFull(sb) => {
                self.params(&sb.params);
                self.fn_body(&sb.body);
            }
            ExprKind::With { bindings, body } => {
                for b in bindings {
                    self.expr(&b.handler);
                }
                self.block(body);
            }
            ExprKind::HandlerLit { methods, .. } | ExprKind::ProtocolLit { methods, .. } => {
                for m in methods {
                    for p in &m.params {
                        self.decl(&p.name, p.span, "parameter");
                    }
                    match &m.body {
                        HandlerMethodBody::Expr(x) => self.expr(x),
                        HandlerMethodBody::Block(b) => self.block(b),
                    }
                }
            }
            ExprKind::Select { arms } => {
                for arm in arms {
                    match &arm.op {
                        SelectOp::Recv { binding, chan, .. } => {
                            if let Some(b) = binding {
                                self.decl(b, arm.span, "binding");
                            }
                            self.expr(chan);
                        }
                        SelectOp::Send { chan, value } => {
                            self.expr(chan);
                            self.expr(value);
                        }
                        SelectOp::Default => {}
                    }
                    if let Some(g) = &arm.guard {
                        self.expr(g);
                    }
                    self.block(&arm.body);
                }
            }
            ExprKind::Forall { var, range, body } | ExprKind::Exists { var, range, body } => {
                self.decl(var, e.span, "quantifier variable");
                self.expr(range);
                self.expr(body);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::COMPILER_NAMESPACES;

    fn parse_err(src: &str) -> Option<String> {
        crate::parser::parse(src).err().map(|d| d.message)
    }

    /// One cell per (kind of declaration) x (reserved namespace): `{n}` is a
    /// lower-case name, `{N}` an upper-case one, both in the namespace.
    const CELLS: &[(&str, &str)] = &[
        ("local ro", "fn f() -> int {\n    ro {n} = 1\n    1\n}\n"),
        ("local mut", "fn f() -> int {\n    mut {n} = 1\n    1\n}\n"),
        ("local consume", "fn f() -> () {\n    consume {n} = g()\n    ()\n}\n"),
        ("consume scope", "fn f() -> () {\n    consume {n} = g() {\n        ()\n    }\n}\n"),
        ("defer outcome", "fn f() -> () {\n    defer({n} ScopeOutcome) {\n        ()\n    }\n}\n"),
        ("local const", "fn f() -> int {\n    const {N} = 1\n    1\n}\n"),
        ("parameter", "fn f({n} int) -> int => 1\n"),
        ("generic parameter", "fn f[{N}](x {N}) -> {N} => x\n"),
        ("function", "fn {n}() -> int => 1\n"),
        ("method", "type R { x int }\nfn R @{n}() -> int => 1\n"),
        ("static method", "type R { x int }\nfn R.{n}() -> int => 1\n"),
        ("type", "type {N} { x int }\n"),
        ("record field", "type R { {n} int }\n"),
        ("value-record field", "type R value { {n} int }\n"),
        ("sum variant", "type S enum\n    | {N}\n    | B\n"),
        ("sum variant field", "type S enum\n    | A { {n} int }\n    | B\n"),
        ("effect", "type {N} effect {\n    fn op(x int) -> int\n}\n"),
        ("effect operation", "type E effect {\n    fn {n}(x int) -> int\n}\n"),
        ("effect operation parameter", "type E effect {\n    fn op({n} int) -> int\n}\n"),
        ("protocol", "type {N} protocol {\n    @area() -> int\n}\n"),
        ("protocol method", "type P protocol {\n    @{n}() -> int\n}\n"),
        ("constant", "const {N} = 1\n"),
        ("module value", "ro {n} = 1\n"),
        ("match pattern", "fn f(o Option[int]) -> int => match o {\n    Some({n}) => 1\n    None => 0\n}\n"),
        ("tuple pattern", "fn f() -> int {\n    ro ({n}, b) = (1, 2)\n    b\n}\n"),
        ("for pattern", "fn f() -> () {\n    for {n} in 0..3 {\n        ()\n    }\n}\n"),
        ("if-let pattern", "fn f(o Option[int]) -> int {\n    if Some({n}) = o {\n        return 1\n    }\n    0\n}\n"),
        ("lambda parameter", "fn f() -> int => g(fn({n} int) -> int => 1)\n"),
        ("light closure parameter", "fn f() -> int => g(|{n}| 1)\n"),
        ("handler parameter", "type E effect {\n    fn op(x int) -> int\n}\nfn f() -> int => with E = effect E { op({n} int) -> int => 1 } { 1 }\n"),
        ("name inside interpolation", "fn f() -> str => \"${g(|{n}| 1)}\"\n"),
    ];

    fn fill(t: &str, p: &str) -> (String, String) {
        let lower = format!("{}q", p);
        let upper = format!("{}Q", p);
        let name = if t.contains("{N}") { upper.clone() } else { lower.clone() };
        (t.replace("{n}", &lower).replace("{N}", &upper), name)
    }

    #[test]
    fn every_kind_in_every_namespace_is_refused() {
        let mut misses = Vec::new();
        for (kind, t) in CELLS {
            for p in COMPILER_NAMESPACES {
                let (src, name) = fill(t, p);
                let src = format!("module m\n{}", src);
                match parse_err(&src) {
                    Some(m) if m.contains("[E_RESERVED_NAME]") && m.contains(&format!("`{}`", name)) => {}
                    other => misses.push(format!("{kind} x {p}: {other:?}")),
                }
            }
        }
        assert!(misses.is_empty(), "cells not refused:\n{}", misses.join("\n"));
    }

    /// The same cells with ordinary names parse: the refusal is the
    /// namespace, not the construct (the control that keeps the grid honest).
    #[test]
    fn same_cells_with_lawful_names_are_not_refused() {
        for (kind, t) in CELLS {
            for p in ["", "_", "nv_", "nova_", "at_", "x__"] {
                let (src, _) = fill(t, p);
                let src = format!("module m\n{}", src);
                if let Some(m) = parse_err(&src) {
                    assert!(!m.contains("E_RESERVED_NAME"), "{kind} x `{p}`: {m}");
                    panic!("{kind} x `{p}` does not parse at all (the cell is broken): {m}");
                }
            }
        }
    }

    /// Names the parser synthesizes in the reserved space are not the
    /// programmer's: `decreases` lowers to `_nova_decr_old`, an anonymous
    /// embed is named `__embed_<T>`. The reference `__other` makes the walk
    /// run at all (a file without a reserved-prefix token is skipped).
    #[test]
    fn parser_synthesized_names_pass() {
        let decr = "module m\nfn f() -> int {\n    mut i = 3\n    while i > 0\n        decreases i\n    {\n        i -= 1\n    }\n    __other\n}\n";
        assert_eq!(parse_err(decr), None);
        let embed = "module m\ntype B { y int }\ntype A {\n    use _ B\n    x int\n}\nfn g() -> int => __other\n";
        assert_eq!(parse_err(embed), None);
    }

    /// D282: the name of an `extern "C"` function is a C library's symbol.
    #[test]
    fn extern_c_symbol_name_passes_but_its_parameters_do_not() {
        assert_eq!(parse_err("module m\nextern \"C\" fn __errno_location() -> int\n"), None);
        let m = parse_err("module m\nextern \"C\" fn c_open(__path int) -> int\n").unwrap_or_default();
        assert!(m.contains("[E_RESERVED_NAME]"), "{m}");
    }

    /// A reference to a reserved-space name is not a declaration: this rule
    /// does not judge it (resolution does).
    #[test]
    fn a_reference_is_not_judged() {
        assert_eq!(parse_err("module m\nfn f() -> int => _nova_x + __y\n"), None);
    }
}
