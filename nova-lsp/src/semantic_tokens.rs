// SPDX-License-Identifier: MIT OR Apache-2.0
//! Full semantic tokens — Plan 104.10 Ф.10 (BLOCK C).
//!
//! Supersedes the narrow Plan 123.5.2 producer (which only tagged *cached*
//! `@field` reads with a single `PROPERTY` legend). This module emits a token
//! for **every meaningful lexeme** in the buffer — keywords, literals,
//! doc-comments, and, crucially, a *semantically classified* token for every
//! identifier (function / method / type / type-parameter / parameter / variable
//! / property / namespace / enum-member).
//!
//! # Architecture — completeness from the lexer, precision from the AST
//!
//! 1. **Lexer stream** ([`nova_codegen::lexer::lex`]) is the token source. It
//!    already carries exact byte spans and distinguishes keywords, string /
//!    numeric / char literals, doc-comments and identifiers. Emitting from it
//!    guarantees *every* lexeme is covered even when the buffer has type errors
//!    (the checker `env` may be `None`); nothing is silently dropped.
//!
//! 2. **AST classification** refines each *identifier* token. The refinement has
//!    two channels:
//!    - an **override map** (`byte-offset → (type, modifiers)`) built by walking
//!      the entry file's declarations (fn / type / param / field / variant /
//!      generic / const names get the `declaration` modifier and their exact
//!      class) plus a per-function scope walk that marks *uses of parameters* as
//!      `parameter` and local binding sites as declarations, plus type-reference
//!      positions (so primitive / imported type names classify as `type` even
//!      when they are not module-local declarations);
//!    - **name-set + lexer-context** fallback at emission time for every
//!      identifier not in the override map: a member (`x.foo`) followed by `(`
//!      is a `method`, otherwise a `property`; a bare name followed by `(` is a
//!      `function` (or `type` / `enumMember` for a constructor); an identifier
//!      whose text matches a declared type / variant / function / const gets
//!      that class; everything else is a `variable`.
//!
//! The cached-`@field` highlight is preserved as a **special case of a
//! modifier**: every `@field` read is a `property`, and the ones the field-cache
//! analysis would fold into a cache local additionally carry the custom `cached`
//! modifier (plus `readonly`) — exactly the Plan 123.5.2 semantics, now layered
//! on the full pass instead of being the only thing emitted.
//!
//! # Delta
//!
//! This module produces the **full** delta-encoded token vector. The incremental
//! `textDocument/semanticTokens/full/delta` edit-script computation lives in
//! [`crate::semantic_tokens_delta`] and is unchanged — the server feeds this
//! vector to `build_delta_response`.
//!
//! # UTF-16 correctness
//!
//! Every token position and length is in **UTF-16 code units**
//! ([`byte_offset_to_position`]), so a token after a multi-byte identifier lands
//! at the correct client column. Multi-line lexemes (a merged `///` doc-comment
//! block, a multi-line string) are split at newline boundaries — the LSP wire
//! format forbids a single token from spanning lines.
//!
//! # Residual — [M-104.10-semantic-tokens-scope]
//!
//! Parameter-use classification is scope-approximate: it matches a use against
//! the *enclosing function's* parameter names without tracking nested shadowing
//! (a closure parameter or `let` binding that reuses a parameter name is still
//! reported as `parameter`). Identifiers inside interpolated-string `${…}`
//! fragments are covered by the surrounding `string` token (the lexer does not
//! split interpolation). Effect/protocol method-signature internals are not
//! walked for the `declaration` modifier. See `simplifications.md` / backlog.

use std::collections::{HashMap, HashSet};

use ropey::Rope;
use tower_lsp::lsp_types::{
    SemanticToken, SemanticTokenModifier, SemanticTokenType,
};

use nova_codegen::ast::{
    ArrayElem, ArrayPatternElem, AssocConst, Block, ClosureBody, ConstDecl, ElseBranch, Expr,
    ExprKind, FnBody, FnDecl, GenericParam, Item, MatchArmBody, Param, Pattern, RecordField, Stmt,
    TypeDecl, TypeDeclKind, TypeRef,
};
use nova_codegen::diag::MAIN_FILE_ID;
use nova_codegen::lexer::{lex, Token, TokenKind};

use crate::diagnostic_mapping::byte_offset_to_position;
use crate::provenance::ResolvedModule;

// ─────────────────────────────────────────────────────────────────────────────
// Legend — token types & modifiers
// ─────────────────────────────────────────────────────────────────────────────
//
// Indices below MUST match the order of these two vectors (LSP encodes tokens by
// legend index). Keep additions append-only so cached client legends stay valid.

const T_NAMESPACE: u32 = 0;
const T_TYPE: u32 = 1;
const T_TYPE_PARAMETER: u32 = 2;
const T_PARAMETER: u32 = 3;
const T_VARIABLE: u32 = 4;
const T_PROPERTY: u32 = 5;
const T_ENUM_MEMBER: u32 = 6;
const T_FUNCTION: u32 = 7;
const T_METHOD: u32 = 8;
const T_KEYWORD: u32 = 9;
const T_COMMENT: u32 = 10;
const T_STRING: u32 = 11;
const T_NUMBER: u32 = 12;

/// Full token-type legend advertised at `initialize`. Index-stable superset of
/// the Plan 123.5.2 single-`PROPERTY` legend.
pub fn semantic_token_legend_types() -> Vec<SemanticTokenType> {
    vec![
        SemanticTokenType::NAMESPACE,      // 0
        SemanticTokenType::TYPE,           // 1
        SemanticTokenType::TYPE_PARAMETER, // 2
        SemanticTokenType::PARAMETER,      // 3
        SemanticTokenType::VARIABLE,       // 4
        SemanticTokenType::PROPERTY,       // 5
        SemanticTokenType::ENUM_MEMBER,    // 6
        SemanticTokenType::FUNCTION,       // 7
        SemanticTokenType::METHOD,         // 8
        SemanticTokenType::KEYWORD,        // 9
        SemanticTokenType::COMMENT,        // 10
        SemanticTokenType::STRING,         // 11
        SemanticTokenType::NUMBER,         // 12
    ]
}

const M_DECLARATION: u32 = 1 << 0;
const M_READONLY: u32 = 1 << 1;
const M_CACHED: u32 = 1 << 2;

/// Full token-modifier legend. Bit positions correspond to indices here.
/// `cached` (bit 2) is the custom Nova modifier that visually distinguishes an
/// `@field` read the compiler would fold into a cache local (Plan 123.5.2 /
/// D217 V1) — preserved as a modifier layered on `property`.
pub fn semantic_token_legend_modifiers() -> Vec<SemanticTokenModifier> {
    vec![
        SemanticTokenModifier::DECLARATION,   // bit 0
        SemanticTokenModifier::READONLY,      // bit 1
        SemanticTokenModifier::new("cached"), // bit 2
    ]
}

// ─────────────────────────────────────────────────────────────────────────────
// Public entry point
// ─────────────────────────────────────────────────────────────────────────────

/// Compute the full delta-encoded semantic-token vector for `src`, using the
/// already-resolved module (Ф.1 cache, provides the AST for classification).
///
/// Never panics and never returns `None`: on a lex error it returns an empty
/// vector (the editor keeps its grammar highlighting); on a checker/parse
/// failure the classification simply degrades to lexer + name-set fallback.
pub fn compute_semantic_tokens(src: &str, resolved: &ResolvedModule) -> Vec<SemanticToken> {
    let tokens = match lex(src) {
        Ok(t) => t,
        Err(_) => return Vec::new(),
    };
    let rope = Rope::from_str(src);
    let cx = Context::build(src, resolved, &tokens);
    let abs = emit(&tokens, &cx, src, &rope);
    encode(abs)
}

// ─────────────────────────────────────────────────────────────────────────────
// Absolute (pre-delta) token
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
struct AbsTok {
    line: u32,
    start: u32, // UTF-16 column
    len: u32,   // UTF-16 units
    ttype: u32,
    mods: u32,
}

// ─────────────────────────────────────────────────────────────────────────────
// Classification context (built from the AST)
// ─────────────────────────────────────────────────────────────────────────────

struct Context {
    /// Exact byte-offset → (token_type, modifiers) for identifiers the AST walk
    /// could attribute authoritatively (declarations, parameter uses, type refs).
    overrides: HashMap<usize, (u32, u32)>,
    /// Module-local declared names, used by the emission fallback.
    type_names: HashSet<String>,
    variant_names: HashSet<String>,
    fn_names: HashSet<String>,
    const_names: HashSet<String>,
    /// Per-function `(start, end, cached-field-names)` — the field-cache analysis
    /// result used to decorate `@field` reads with the `cached` modifier.
    cached_per_fn: Vec<(usize, usize, HashSet<String>)>,
}

impl Context {
    fn build(src: &str, resolved: &ResolvedModule, tokens: &[Token]) -> Self {
        // Pre-extract identifier tokens sorted by start offset — used to locate
        // bare-`String` declaration names inside their (wider) declaration spans
        // without fragile substring search.
        let ident_tokens: Vec<(usize, usize, &str)> = tokens
            .iter()
            .filter_map(|t| match &t.kind {
                TokenKind::Ident(s) => Some((t.span.start, t.span.end, s.as_str())),
                _ => None,
            })
            .collect();

        let mut cx = Context {
            overrides: HashMap::new(),
            type_names: HashSet::new(),
            variant_names: HashSet::new(),
            fn_names: HashSet::new(),
            const_names: HashSet::new(),
            cached_per_fn: cached_field_spans(src),
        };

        let module = &resolved.module;
        let start = resolved.items_start.min(module.items.len());
        // First pass: collect declared names for the emission fallback.
        for item in &module.items[start..] {
            cx.collect_names(item);
        }
        // Second pass: record precise overrides.
        let idents = &ident_tokens;
        for item in &module.items[start..] {
            cx.record_item(item, idents);
        }
        cx
    }

    // ── name-set collection ────────────────────────────────────────────────────

    fn collect_names(&mut self, item: &Item) {
        match item {
            Item::Fn(fd) => {
                if fd.receiver.is_none() {
                    self.fn_names.insert(fd.name.clone());
                }
            }
            Item::Type(td) => {
                self.type_names.insert(td.name.clone());
                if let TypeDeclKind::Sum(vs) = &td.kind {
                    for v in vs {
                        self.variant_names.insert(v.name.clone());
                    }
                }
            }
            Item::Const(cd) => {
                self.const_names.insert(cd.name.clone());
            }
            Item::Let(ld) => {
                if let Pattern::Ident { name, .. } = &ld.pattern {
                    self.const_names.insert(name.clone());
                }
            }
            _ => {}
        }
    }

    // ── override recording ─────────────────────────────────────────────────────

    fn put(&mut self, off: usize, class: (u32, u32)) {
        self.overrides.insert(off, class);
    }

    fn record_item(&mut self, item: &Item, idents: &[(usize, usize, &str)]) {
        match item {
            Item::Fn(fd) => self.record_fn(fd, idents),
            Item::Type(td) => self.record_type(td, idents),
            Item::Const(cd) => self.record_const(cd, idents),
            Item::Let(ld) => {
                let readonly = if ld.mutable { 0 } else { M_READONLY };
                self.record_binding_pattern(&ld.pattern, idents, M_DECLARATION | readonly);
                if let Some(ty) = &ld.ty {
                    self.record_typeref(ty, idents, &HashSet::new());
                }
                let empty = HashSet::new();
                self.walk_expr(&ld.value, &empty, idents);
            }
            Item::Test(td) => {
                let empty = HashSet::new();
                self.walk_block(&td.body, &empty, idents);
            }
            _ => {}
        }
    }

    fn record_fn(&mut self, fd: &FnDecl, idents: &[(usize, usize, &str)]) {
        let span = (fd.span.start as usize, fd.span.end as usize);
        // Generic type-parameter names in scope for this fn (so their uses in
        // parameter/return types classify as `typeParameter`, not `type`).
        let type_params: HashSet<String> =
            fd.generics.iter().map(|g| g.name.clone()).collect();

        // Receiver type name (a reference, no `declaration`).
        if let Some(rcv) = &fd.receiver {
            if let Some(off) = first_ident_in(idents, span.0, span.1, &rcv.type_name) {
                let class = if type_params.contains(&rcv.type_name) {
                    T_TYPE_PARAMETER
                } else {
                    T_TYPE
                };
                self.put(off, (class, 0));
            }
        }
        // Function / method name (declaration).
        let name_class = if fd.receiver.is_some() { T_METHOD } else { T_FUNCTION };
        if let Some(off) = first_ident_in(idents, span.0, span.1, &fd.name) {
            self.put(off, (name_class, M_DECLARATION));
        }
        // Generic parameter declarations + their bounds.
        self.record_generics(&fd.generics, idents, span, &type_params);

        // Parameter names (declaration) + parameter types.
        let mut param_names: HashSet<String> = HashSet::new();
        for p in &fd.params {
            param_names.insert(p.name.clone());
            self.record_param(p, idents, &type_params);
        }
        if let Some(rt) = &fd.return_type {
            self.record_typeref(rt, idents, &type_params);
        }
        for p in &fd.params {
            if let Some(def) = &p.default {
                self.walk_expr(def, &param_names, idents);
            }
        }
        // Body: parameter-use + local-binding classification.
        match &fd.body {
            FnBody::Block(b) => self.walk_block(b, &param_names, idents),
            FnBody::Expr(e) => self.walk_expr(e, &param_names, idents),
            FnBody::External => {}
        }
    }

    fn record_generics(
        &mut self,
        generics: &[GenericParam],
        idents: &[(usize, usize, &str)],
        span: (usize, usize),
        type_params: &HashSet<String>,
    ) {
        for g in generics {
            if let Some(off) = first_ident_in(idents, span.0, span.1, &g.name) {
                self.put(off, (T_TYPE_PARAMETER, M_DECLARATION));
            }
            for b in &g.bounds {
                self.record_typeref(b, idents, type_params);
            }
            if let Some(d) = &g.default {
                self.record_typeref(d, idents, type_params);
            }
        }
    }

    fn record_param(
        &mut self,
        p: &Param,
        idents: &[(usize, usize, &str)],
        type_params: &HashSet<String>,
    ) {
        let sp = (p.span.start as usize, p.span.end as usize);
        if let Some(off) = first_ident_in(idents, sp.0, sp.1, &p.name) {
            self.put(off, (T_PARAMETER, M_DECLARATION));
        }
        self.record_typeref(&p.ty, idents, type_params);
    }

    fn record_type(&mut self, td: &TypeDecl, idents: &[(usize, usize, &str)]) {
        let span = (td.span.start as usize, td.span.end as usize);
        let type_params: HashSet<String> =
            td.generics.iter().map(|g| g.name.clone()).collect();
        if let Some(off) = first_ident_in(idents, span.0, span.1, &td.name) {
            self.put(off, (T_TYPE, M_DECLARATION));
        }
        self.record_generics(&td.generics, idents, span, &type_params);

        match &td.kind {
            TypeDeclKind::Record(fields) => {
                for f in fields {
                    self.record_field(f, idents, &type_params);
                }
            }
            TypeDeclKind::NamedTuple(fields) => {
                for f in fields {
                    let fsp = (f.span.start as usize, f.span.end as usize);
                    if let Some(off) = first_ident_in(idents, fsp.0, fsp.1, &f.name) {
                        self.put(off, (T_PROPERTY, M_DECLARATION));
                    }
                    self.record_typeref(&f.ty, idents, &type_params);
                }
            }
            TypeDeclKind::Sum(variants) => {
                for v in variants {
                    let vsp = (v.span.start as usize, v.span.end as usize);
                    if let Some(off) = first_ident_in(idents, vsp.0, vsp.1, &v.name) {
                        self.put(off, (T_ENUM_MEMBER, M_DECLARATION));
                    }
                }
            }
            TypeDeclKind::Newtype(tr)
            | TypeDeclKind::Alias(tr) => self.record_typeref(tr, idents, &type_params),
            TypeDeclKind::TypeSet(trs) => {
                for tr in trs {
                    self.record_typeref(tr, idents, &type_params);
                }
            }
            TypeDeclKind::Protocol { embeds, .. } => {
                for tr in embeds {
                    self.record_typeref(tr, idents, &type_params);
                }
            }
            TypeDeclKind::Effect(_) | TypeDeclKind::Opaque => {}
        }
        for ac in &td.assoc_consts {
            self.record_assoc_const(ac, idents, &type_params);
        }
    }

    fn record_field(
        &mut self,
        f: &RecordField,
        idents: &[(usize, usize, &str)],
        type_params: &HashSet<String>,
    ) {
        let fsp = (f.span.start as usize, f.span.end as usize);
        let readonly = if f.readonly { M_READONLY } else { 0 };
        if let Some(off) = first_ident_in(idents, fsp.0, fsp.1, &f.name) {
            self.put(off, (T_PROPERTY, M_DECLARATION | readonly));
        }
        self.record_typeref(&f.ty, idents, type_params);
    }

    fn record_assoc_const(
        &mut self,
        ac: &AssocConst,
        idents: &[(usize, usize, &str)],
        type_params: &HashSet<String>,
    ) {
        let sp = (ac.span.start as usize, ac.span.end as usize);
        if let Some(off) = first_ident_in(idents, sp.0, sp.1, &ac.name) {
            self.put(off, (T_VARIABLE, M_DECLARATION | M_READONLY));
        }
        if let Some(ty) = &ac.ty {
            self.record_typeref(ty, idents, type_params);
        }
        let empty = HashSet::new();
        self.walk_expr(&ac.value, &empty, idents);
    }

    fn record_const(&mut self, cd: &ConstDecl, idents: &[(usize, usize, &str)]) {
        let sp = (cd.span.start as usize, cd.span.end as usize);
        if let Some(off) = first_ident_in(idents, sp.0, sp.1, &cd.name) {
            self.put(off, (T_VARIABLE, M_DECLARATION | M_READONLY));
        }
        let empty = HashSet::new();
        if let Some(ty) = &cd.ty {
            self.record_typeref(ty, idents, &empty);
        }
        self.walk_expr(&cd.value, &empty, idents);
    }

    // ── type references ────────────────────────────────────────────────────────

    fn record_typeref(
        &mut self,
        tr: &TypeRef,
        idents: &[(usize, usize, &str)],
        type_params: &HashSet<String>,
    ) {
        match tr {
            TypeRef::Named { path, generics, span } => {
                let mut lo = span.start as usize;
                let hi = span.end as usize;
                let last = path.len().saturating_sub(1);
                for (i, seg) in path.iter().enumerate() {
                    if let Some(off) = first_ident_in(idents, lo, hi, seg) {
                        let class = if i == last {
                            if type_params.contains(seg) { T_TYPE_PARAMETER } else { T_TYPE }
                        } else {
                            T_NAMESPACE
                        };
                        self.put(off, (class, 0));
                        lo = off + seg.len();
                    }
                }
                for g in generics {
                    self.record_typeref(g, idents, type_params);
                }
            }
            TypeRef::Array(inner, _)
            | TypeRef::FixedArray(_, inner, _)
            | TypeRef::Readonly(inner, _)
            | TypeRef::Mut(inner, _)
            | TypeRef::Uninit(inner, _)
            | TypeRef::Pointer(inner, _)
            | TypeRef::Ref(inner, _) => self.record_typeref(inner, idents, type_params),
            TypeRef::Tuple(elems, _) => {
                for e in elems {
                    self.record_typeref(e, idents, type_params);
                }
            }
            TypeRef::Func { params, effects, return_type, .. } => {
                for p in params {
                    self.record_typeref(p, idents, type_params);
                }
                for e in effects {
                    self.record_typeref(e, idents, type_params);
                }
                if let Some(rt) = return_type {
                    self.record_typeref(rt, idents, type_params);
                }
            }
            TypeRef::Protocol { .. } | TypeRef::Unit(_) => {}
        }
    }

    // ── scope walk: parameter uses + local binding declarations ─────────────────

    fn record_binding_pattern(
        &mut self,
        pat: &Pattern,
        idents: &[(usize, usize, &str)],
        mods: u32,
    ) {
        match pat {
            Pattern::Ident { name, span, .. } => {
                let sp = (span.start as usize, span.end as usize);
                if let Some(off) = first_ident_in(idents, sp.0, sp.1, name) {
                    self.put(off, (T_VARIABLE, mods));
                }
            }
            Pattern::Tuple(ps, _) => {
                for p in ps {
                    self.record_binding_pattern(p, idents, mods);
                }
            }
            Pattern::Array { elems, .. } => {
                for e in elems {
                    match e {
                        ArrayPatternElem::Item(p) => self.record_binding_pattern(p, idents, mods),
                        ArrayPatternElem::RestBind(name) => {
                            // Best-effort: rest-bind carries only a name, no span.
                            let _ = name;
                        }
                        ArrayPatternElem::Rest => {}
                    }
                }
            }
            Pattern::Record { fields, .. } => {
                for f in fields {
                    match &f.pattern {
                        Some(p) => self.record_binding_pattern(p, idents, mods),
                        None => {
                            let fsp = (f.span.start as usize, f.span.end as usize);
                            if let Some(off) = first_ident_in(idents, fsp.0, fsp.1, &f.name) {
                                self.put(off, (T_VARIABLE, mods));
                            }
                        }
                    }
                }
            }
            Pattern::Variant { kind, .. } => {
                if let nova_codegen::ast::VariantPatternKind::Tuple { patterns, .. } = kind {
                    for p in patterns {
                        self.record_binding_pattern(p, idents, mods);
                    }
                }
            }
            Pattern::Binding { inner, .. } => self.record_binding_pattern(inner, idents, mods),
            Pattern::Or { alternatives, .. } => {
                for p in alternatives {
                    self.record_binding_pattern(p, idents, mods);
                }
            }
            Pattern::Wildcard(_) | Pattern::Literal(_, _) => {}
        }
    }

    fn walk_block(
        &mut self,
        block: &Block,
        params: &HashSet<String>,
        idents: &[(usize, usize, &str)],
    ) {
        for s in &block.stmts {
            self.walk_stmt(s, params, idents);
        }
        if let Some(t) = &block.trailing {
            self.walk_expr(t, params, idents);
        }
    }

    fn walk_stmt(
        &mut self,
        stmt: &Stmt,
        params: &HashSet<String>,
        idents: &[(usize, usize, &str)],
    ) {
        match stmt {
            Stmt::Let(ld) => {
                let readonly = if ld.mutable { 0 } else { M_READONLY };
                self.record_binding_pattern(&ld.pattern, idents, M_DECLARATION | readonly);
                if let Some(ty) = &ld.ty {
                    self.record_typeref(ty, idents, &HashSet::new());
                }
                self.walk_expr(&ld.value, params, idents);
            }
            Stmt::Const(cd) => {
                let sp = (cd.span.start as usize, cd.span.end as usize);
                if let Some(off) = first_ident_in(idents, sp.0, sp.1, &cd.name) {
                    self.put(off, (T_VARIABLE, M_DECLARATION | M_READONLY));
                }
                self.walk_expr(&cd.value, params, idents);
            }
            Stmt::Expr(e) => self.walk_expr(e, params, idents),
            Stmt::Assign { target, value, .. } => {
                self.walk_expr(target, params, idents);
                self.walk_expr(value, params, idents);
            }
            Stmt::TupleAssign { lhs, rhs, .. } => {
                for e in lhs {
                    self.walk_expr(e, params, idents);
                }
                for e in rhs {
                    self.walk_expr(e, params, idents);
                }
            }
            Stmt::Return { value, .. } => {
                if let Some(e) = value {
                    self.walk_expr(e, params, idents);
                }
            }
            Stmt::Throw { value, .. } => self.walk_expr(value, params, idents),
            Stmt::Defer { body, .. } => self.walk_expr(body, params, idents),
            Stmt::ConsumeScope { init, body, .. } => {
                self.walk_expr(init, params, idents);
                self.walk_block(body, params, idents);
            }
            Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => {
                self.walk_expr(expr, params, idents)
            }
            Stmt::Apply { args, .. } => {
                for a in args {
                    self.walk_expr(a, params, idents);
                }
            }
            Stmt::Calc { steps, .. } => {
                for s in steps {
                    self.walk_expr(&s.expr, params, idents);
                }
            }
            _ => {}
        }
    }

    fn walk_expr(
        &mut self,
        expr: &Expr,
        params: &HashSet<String>,
        idents: &[(usize, usize, &str)],
    ) {
        match &expr.kind {
            ExprKind::Ident(n) => {
                if params.contains(n) && expr.span.file_id == MAIN_FILE_ID {
                    self.put(expr.span.start as usize, (T_PARAMETER, 0));
                }
            }
            ExprKind::Call { func, args, trailing } => {
                self.walk_expr(func, params, idents);
                for a in args {
                    self.walk_expr(a.expr(), params, idents);
                }
                if let Some(t) = trailing {
                    self.walk_trailing(t, params, idents);
                }
            }
            ExprKind::For { pattern, iter, body, .. }
            | ExprKind::ParallelFor { pattern, iter, body, .. } => {
                self.record_binding_pattern(pattern, idents, M_DECLARATION);
                self.walk_expr(iter, params, idents);
                self.walk_block(body, params, idents);
            }
            ExprKind::IfLet { pattern, scrutinee, guard, then, else_, .. } => {
                self.record_binding_pattern(pattern, idents, M_DECLARATION);
                self.walk_expr(scrutinee, params, idents);
                if let Some(g) = guard {
                    self.walk_expr(g, params, idents);
                }
                self.walk_block(then, params, idents);
                self.walk_else(else_, params, idents);
            }
            ExprKind::WhileLet { pattern, scrutinee, guard, body, .. } => {
                self.record_binding_pattern(pattern, idents, M_DECLARATION);
                self.walk_expr(scrutinee, params, idents);
                if let Some(g) = guard {
                    self.walk_expr(g, params, idents);
                }
                self.walk_block(body, params, idents);
            }
            ExprKind::If { cond, then, else_, .. } => {
                self.walk_expr(cond, params, idents);
                self.walk_block(then, params, idents);
                self.walk_else(else_, params, idents);
            }
            ExprKind::While { cond, body, .. } => {
                self.walk_expr(cond, params, idents);
                self.walk_block(body, params, idents);
            }
            ExprKind::Loop { body, .. } => self.walk_block(body, params, idents),
            ExprKind::Block(b) => self.walk_block(b, params, idents),
            ExprKind::Match { scrutinee, arms, .. } => {
                self.walk_expr(scrutinee, params, idents);
                for arm in arms {
                    self.record_binding_pattern(&arm.pattern, idents, M_DECLARATION);
                    if let Some(g) = &arm.guard {
                        self.walk_expr(g, params, idents);
                    }
                    match &arm.body {
                        MatchArmBody::Expr(e) => self.walk_expr(e, params, idents),
                        MatchArmBody::Block(b) => self.walk_block(b, params, idents),
                    }
                }
            }
            ExprKind::ClosureLight { body, .. } => match body {
                ClosureBody::Expr(e) => self.walk_expr(e, params, idents),
                ClosureBody::Block(b) => self.walk_block(b, params, idents),
            },
            ExprKind::ClosureFull(sig_body) => match &sig_body.body {
                FnBody::Block(b) => self.walk_block(b, params, idents),
                FnBody::Expr(e) => self.walk_expr(e, params, idents),
                FnBody::External => {}
            },
            ExprKind::Lambda { body, .. } => self.walk_expr(body, params, idents),
            ExprKind::Member { obj, .. } => self.walk_expr(obj, params, idents),
            ExprKind::Index { obj, index } => {
                self.walk_expr(obj, params, idents);
                self.walk_expr(index, params, idents);
            }
            ExprKind::Binary { left, right, .. } => {
                self.walk_expr(left, params, idents);
                self.walk_expr(right, params, idents);
            }
            ExprKind::Unary { operand, .. } => self.walk_expr(operand, params, idents),
            ExprKind::TupleLit(elems) => {
                for e in elems {
                    self.walk_expr(e, params, idents);
                }
            }
            ExprKind::ArrayLit(elems) => {
                for e in elems {
                    match e {
                        ArrayElem::Item(x) | ArrayElem::Spread(x) => {
                            self.walk_expr(x, params, idents)
                        }
                    }
                }
            }
            ExprKind::MapLit { elems, .. } => {
                for e in elems {
                    match e {
                        nova_codegen::ast::MapElem::Pair(k, v) => {
                            self.walk_expr(k, params, idents);
                            self.walk_expr(v, params, idents);
                        }
                        nova_codegen::ast::MapElem::Spread(x) => self.walk_expr(x, params, idents),
                    }
                }
            }
            ExprKind::RecordLit { fields, .. } => {
                for f in fields {
                    if let Some(v) = &f.value {
                        self.walk_expr(v, params, idents);
                    }
                }
            }
            ExprKind::TurboFish { base, .. } => self.walk_expr(base, params, idents),
            ExprKind::Forbid { body, .. } | ExprKind::Blocking(body) | ExprKind::Detach(body) => {
                self.walk_block(body, params, idents)
            }
            ExprKind::Realtime { body, .. } => self.walk_block(body, params, idents),
            ExprKind::Supervised { body, cancel, .. } => {
                self.walk_block(body, params, idents);
                if let Some(c) = cancel {
                    self.walk_expr(c, params, idents);
                }
            }
            ExprKind::With { body, bindings } => {
                for b in bindings {
                    self.walk_expr(&b.handler, params, idents);
                }
                self.walk_block(body, params, idents);
            }
            ExprKind::Try(inner)
            | ExprKind::Bang(inner)
            | ExprKind::Spawn(inner)
            | ExprKind::Throw(inner) => self.walk_expr(inner, params, idents),
            ExprKind::Interrupt(inner) => {
                if let Some(e) = inner {
                    self.walk_expr(e, params, idents);
                }
            }
            ExprKind::As(inner, ty) | ExprKind::Is(inner, ty) => {
                self.walk_expr(inner, params, idents);
                self.record_typeref(ty, idents, &HashSet::new());
            }
            ExprKind::Coalesce(a, b) => {
                self.walk_expr(a, params, idents);
                self.walk_expr(b, params, idents);
            }
            ExprKind::Range { start, end, .. } => {
                if let Some(e) = start {
                    self.walk_expr(e, params, idents);
                }
                if let Some(e) = end {
                    self.walk_expr(e, params, idents);
                }
            }
            ExprKind::Forall { range, body, .. } | ExprKind::Exists { range, body, .. } => {
                self.walk_expr(range, params, idents);
                self.walk_expr(body, params, idents);
            }
            ExprKind::InterpolatedStr { parts } => {
                for p in parts {
                    if let nova_codegen::ast::InterpStrPart::Expr { expr, .. } = p {
                        self.walk_expr(expr, params, idents);
                    }
                }
            }
            ExprKind::TaggedTemplate { tag, args, .. } => {
                self.walk_expr(tag, params, idents);
                for a in args {
                    self.walk_expr(a, params, idents);
                }
            }
            _ => {}
        }
    }

    fn walk_else(
        &mut self,
        else_: &Option<ElseBranch>,
        params: &HashSet<String>,
        idents: &[(usize, usize, &str)],
    ) {
        match else_ {
            Some(ElseBranch::Block(b)) => self.walk_block(b, params, idents),
            Some(ElseBranch::If(e)) => self.walk_expr(e, params, idents),
            None => {}
        }
    }

    fn walk_trailing(
        &mut self,
        t: &nova_codegen::ast::Trailing,
        params: &HashSet<String>,
        idents: &[(usize, usize, &str)],
    ) {
        use nova_codegen::ast::Trailing;
        match t {
            Trailing::Block(b) => self.walk_block(b, params, idents),
            Trailing::Fn(sig) => match &sig.body {
                FnBody::Block(b) => self.walk_block(b, params, idents),
                FnBody::Expr(e) => self.walk_expr(e, params, idents),
                FnBody::External => {}
            },
            Trailing::LegacyBlockWithParams(tb) => self.walk_block(&tb.body, params, idents),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Emission — lexer stream → absolute tokens
// ─────────────────────────────────────────────────────────────────────────────

fn emit(tokens: &[Token], cx: &Context, src: &str, rope: &Rope) -> Vec<AbsTok> {
    // Significant tokens only (drop `Newline`) so prev/next context skips blank
    // structure. We still track import-line context via a per-line flag.
    let sig: Vec<&Token> = tokens
        .iter()
        .filter(|t| !matches!(t.kind, TokenKind::Newline | TokenKind::Eof))
        .collect();

    let mut out: Vec<AbsTok> = Vec::with_capacity(sig.len());
    // Track whether the current logical line is an `import` / `use` / `module`
    // header, where every identifier is a namespace segment.
    let import_lines = import_line_set(tokens, rope);

    let mut i = 0usize;
    while i < sig.len() {
        let tok = sig[i];
        match &tok.kind {
            TokenKind::At => {
                // `@ident` — self field / method access. Emit ONE token spanning
                // `@`+name so the cached-field highlight (Plan 123.5.2) is
                // preserved as a modifier on the whole `@field` lexeme.
                if let Some(next) = sig.get(i + 1) {
                    if let TokenKind::Ident(name) = &next.kind {
                        if next.span.start == tok.span.end {
                            let followed_by_call =
                                matches!(sig.get(i + 2).map(|t| &t.kind), Some(TokenKind::LParen));
                            let off = next.span.start;
                            let (ttype, mut mods) = match cx.overrides.get(&off) {
                                Some(c) => *c,
                                None if followed_by_call => (T_METHOD, 0),
                                None => (T_PROPERTY, 0),
                            };
                            // Cached-field decoration only for property reads.
                            if ttype == T_PROPERTY && is_cached(cx, off, name) {
                                mods |= M_READONLY | M_CACHED;
                            }
                            push_span(src, rope, tok.span.start, next.span.end, ttype, mods, &mut out);
                            i += 2;
                            continue;
                        }
                    }
                }
                i += 1;
            }
            TokenKind::Ident(name) => {
                let off = tok.span.start;
                let line = byte_offset_to_position(rope, off).line;
                let (ttype, mods) = if let Some(c) = cx.overrides.get(&off) {
                    *c
                } else if import_lines.contains(&line) {
                    (T_NAMESPACE, 0)
                } else {
                    let prev = if i > 0 { Some(&sig[i - 1].kind) } else { None };
                    let next = sig.get(i + 1).map(|t| &t.kind);
                    classify_ident(name, prev, next, cx)
                };
                push_span(src, rope, tok.span.start, tok.span.end, ttype, mods, &mut out);
                i += 1;
            }
            TokenKind::Str(_) | TokenKind::Backtick(_) => {
                push_span(src, rope, tok.span.start, tok.span.end, T_STRING, 0, &mut out);
                i += 1;
            }
            TokenKind::Int(_) | TokenKind::Float(_) | TokenKind::Char(_) => {
                push_span(src, rope, tok.span.start, tok.span.end, T_NUMBER, 0, &mut out);
                i += 1;
            }
            TokenKind::DocComment { .. } => {
                push_span(src, rope, tok.span.start, tok.span.end, T_COMMENT, 0, &mut out);
                i += 1;
            }
            _ => {
                if is_keyword(&tok.kind) {
                    push_span(src, rope, tok.span.start, tok.span.end, T_KEYWORD, 0, &mut out);
                }
                i += 1;
            }
        }
    }
    out
}

/// Classify a bare identifier (not `@ident`, not a declaration override) using
/// its immediate lexer context plus the module's declared-name sets.
fn classify_ident(
    name: &str,
    prev: Option<&TokenKind>,
    next: Option<&TokenKind>,
    cx: &Context,
) -> (u32, u32) {
    let is_member = matches!(prev, Some(TokenKind::Dot));
    let followed_by_call = matches!(next, Some(TokenKind::LParen));

    if is_member {
        if followed_by_call {
            return (T_METHOD, 0);
        }
        // `mod.Type` static access, `Enum.Variant`, else a field/property.
        if cx.type_names.contains(name) {
            return (T_TYPE, 0);
        }
        if cx.variant_names.contains(name) {
            return (T_ENUM_MEMBER, 0);
        }
        return (T_PROPERTY, 0);
    }

    if followed_by_call {
        // Constructor-ish call `Type(...)` / `Variant(...)` vs free function.
        if cx.type_names.contains(name) {
            return (T_TYPE, 0);
        }
        if cx.variant_names.contains(name) {
            return (T_ENUM_MEMBER, 0);
        }
        return (T_FUNCTION, 0);
    }

    if cx.type_names.contains(name) {
        return (T_TYPE, 0);
    }
    if cx.variant_names.contains(name) {
        return (T_ENUM_MEMBER, 0);
    }
    if cx.fn_names.contains(name) {
        return (T_FUNCTION, 0);
    }
    if cx.const_names.contains(name) {
        return (T_VARIABLE, M_READONLY);
    }
    (T_VARIABLE, 0)
}

/// True when byte-offset `off` lies inside a function whose field-cache analysis
/// marked `name` as a cached field.
fn is_cached(cx: &Context, off: usize, name: &str) -> bool {
    cx.cached_per_fn
        .iter()
        .any(|(s, e, set)| off >= *s && off <= *e && set.contains(name))
}

// ─────────────────────────────────────────────────────────────────────────────
// Span → line-split UTF-16 tokens
// ─────────────────────────────────────────────────────────────────────────────

/// Push one or more `AbsTok`s covering byte range `[start, end)`, split at any
/// newline (LSP tokens cannot span lines). Zero-length segments are skipped.
fn push_span(
    src: &str,
    rope: &Rope,
    start: usize,
    end: usize,
    ttype: u32,
    mods: u32,
    out: &mut Vec<AbsTok>,
) {
    let bytes = src.as_bytes();
    let end = end.min(bytes.len());
    let mut seg_start = start;
    let mut i = start;
    while i < end {
        if bytes[i] == b'\n' {
            push_line(rope, seg_start, i, ttype, mods, out);
            seg_start = i + 1;
        }
        i += 1;
    }
    push_line(rope, seg_start, end, ttype, mods, out);
}

fn push_line(rope: &Rope, start: usize, end: usize, ttype: u32, mods: u32, out: &mut Vec<AbsTok>) {
    if end <= start {
        return;
    }
    let sp = byte_offset_to_position(rope, start);
    let ep = byte_offset_to_position(rope, end);
    // Both endpoints are on the same line by construction of push_span.
    if ep.character <= sp.character {
        return;
    }
    out.push(AbsTok {
        line: sp.line,
        start: sp.character,
        len: ep.character - sp.character,
        ttype,
        mods,
    });
}

// ─────────────────────────────────────────────────────────────────────────────
// Sort + overlap-dedupe + delta-encode
// ─────────────────────────────────────────────────────────────────────────────

fn encode(mut abs: Vec<AbsTok>) -> Vec<SemanticToken> {
    abs.sort_by(|a, b| a.line.cmp(&b.line).then(a.start.cmp(&b.start)));

    let mut out: Vec<SemanticToken> = Vec::with_capacity(abs.len());
    let mut prev_line = 0u32;
    let mut prev_start = 0u32;
    let mut last_line = u32::MAX;
    let mut last_end = 0u32; // UTF-16 column just past the previously emitted token
    for t in abs {
        // Drop a token that overlaps the previously emitted one on the same line
        // (defensive — construction is disjoint, but overrides could double-map).
        if t.line == last_line && t.start < last_end {
            continue;
        }
        let dl = t.line - prev_line;
        let ds = if dl == 0 { t.start - prev_start } else { t.start };
        out.push(SemanticToken {
            delta_line: dl,
            delta_start: ds,
            length: t.len,
            token_type: t.ttype,
            token_modifiers_bitset: t.mods,
        });
        prev_line = t.line;
        prev_start = t.start;
        last_line = t.line;
        last_end = t.start + t.len;
    }
    out
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

/// First identifier token with text `name` whose start lies in `[lo, hi)`.
/// `idents` is sorted by start offset. Whole-token match avoids the substring
/// collisions a naive `str::find` would hit (e.g. `c` inside `const`).
fn first_ident_in(idents: &[(usize, usize, &str)], lo: usize, hi: usize, name: &str) -> Option<usize> {
    let from = idents.partition_point(|(s, _, _)| *s < lo);
    for &(s, _, text) in &idents[from..] {
        if s >= hi {
            break;
        }
        if text == name {
            return Some(s);
        }
    }
    None
}

/// Set of 0-based line numbers whose logical line begins an `import` / `use` /
/// `module` header — every identifier on such a line is a namespace segment.
fn import_line_set(tokens: &[Token], rope: &Rope) -> HashSet<u32> {
    let mut set = HashSet::new();
    let mut line_start = true;
    for t in tokens {
        match &t.kind {
            TokenKind::Newline => line_start = true,
            TokenKind::KwImport | TokenKind::KwModule if line_start => {
                let line = byte_offset_to_position(rope, t.span.start).line;
                set.insert(line);
                line_start = false;
            }
            // Plan 239 (D443): `use` — контекстный identifier (был `KwUse`).
            // Сохраняет прежнее поведение: любой line-start `use` считается
            // import/embed-header, как и раньше — `use` лексился как `KwUse`
            // независимо от смысла (import-synonym / record-embed /
            // protocol-embed), так что embed-строки и тогда попадали сюда.
            TokenKind::Ident(s) if s == "use" && line_start => {
                let line = byte_offset_to_position(rope, t.span.start).line;
                set.insert(line);
                line_start = false;
            }
            TokenKind::Eof => {}
            _ => line_start = false,
        }
    }
    set
}

fn is_keyword(kind: &TokenKind) -> bool {
    use TokenKind::*;
    matches!(
        kind,
        KwModule | KwImport | KwExport | KwExternal | KwExtern | KwFn | KwType
            | KwProtocol | KwEffect | KwAlias | KwLet | KwConst | KwMut | KwConsume | KwRo
            | KwReadonly | KwUnsafe | KwUninit | KwSafe | KwPriv | KwPub | KwIf | KwElse | KwMatch | KwFor
            | KwWhile | KwLoop | KwIn | KwReturn | KwBreak | KwContinue | KwTest | KwTrue
            | KwFalse | KwWith | KwThrow | KwAs | KwIs | KwSpawn | KwSupervised | KwParallel
            | KwDetach | KwBlocking | KwInterrupt | KwForbid | KwRealtime | KwAnd | KwOr | KwNot
            | KwDefer | KwErrDefer | KwOkDefer | KwSelect | KwLemma | KwApply
    )
}

// ─────────────────────────────────────────────────────────────────────────────
// Field-cache span extraction (reused from Plan 123.5.2 analysis)
// ─────────────────────────────────────────────────────────────────────────────

/// Per-function `(start, end, cached-field-names)` computed by re-running the
/// field-cache pipeline on a fresh parse of `src`. Mirrors the first half of
/// `server::compute_field_cache_semantic_tokens` so the `cached` modifier stays
/// byte-for-byte compatible with the Plan 123.5.2 behaviour. Returns an empty
/// vec on any parse / check failure (no cached decoration, never a panic).
fn cached_field_spans(src: &str) -> Vec<(usize, usize, HashSet<String>)> {
    let mut module = match crate::compiler::parse_guarded(src) {
        Ok(m) => m,
        Err(_) => return Vec::new(),
    };
    // Plan 181 (D347): alpha-rename before the pipeline so field-cache analysis
    // sees the same unique-named AST as the real build. No-op without a rebind.
    nova_codegen::alpha_rename::alpha_rename(&mut module);
    if nova_codegen::types::check_module(&module).is_err() {
        return Vec::new();
    }
    let _ = nova_codegen::const_fn_eval::rewrite_const_fn_calls(&mut module);
    nova_codegen::types::annotate_map_literals(&mut module);
    nova_codegen::desugar::desugar_module(&mut module);
    nova_codegen::types::infer_effects(&mut module);
    nova_codegen::callnorm::normalize_module(&mut module, &std::collections::HashMap::new());
    // Plan 184 (Р7): IDE-analysis path — no binary is produced, so the
    // value-root guard is irrelevant here; the empty map preserves the
    // pre-184 hoisting shape the field-cache report inspects.
    nova_codegen::chain_norm::normalize_chains_module(
        &mut module, &std::collections::HashMap::new());
    let cfg = nova_codegen::field_cache::FieldCacheConfig::from_env_or_default();
    let report = nova_codegen::field_cache::analyze_module(&module, &cfg);

    let mut out = Vec::new();
    for info in &report.per_fn {
        let mut set: HashSet<String> = HashSet::new();
        for f in &info.ro_caches {
            set.insert(f.clone());
        }
        for f in &info.mut_caches {
            set.insert(f.clone());
        }
        for f in &info.licm_hoists {
            set.insert(f.clone());
        }
        for p in &info.chain_caches {
            if let Some(root) = p.first() {
                set.insert(root.clone());
            }
        }
        if !set.is_empty() {
            out.push((info.span.start as usize, info.span.end as usize, set));
        }
    }
    out
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provenance::resolve_module_for_ide;
    use crate::semantic_tokens_delta::compute_semantic_token_edits;
    use std::path::PathBuf;

    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("nova-lsp has a parent")
            .to_path_buf()
    }

    fn write_temp(stem: &str, src: &str) -> PathBuf {
        let dir = repo_root().join("target").join("semtok_test").join(stem);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("{stem}.nv"));
        std::fs::write(&path, src).unwrap();
        path
    }

    fn tokens(stem: &str, src: &str) -> Vec<SemanticToken> {
        let path = write_temp(stem, src);
        let resolved = resolve_module_for_ide(&path, src);
        compute_semantic_tokens(src, &resolved)
    }

    /// Decode a delta-encoded token vector back into absolute
    /// `(text, token_type, modifiers)` triples. ASCII-only fixtures, so a
    /// UTF-16 column equals a byte column.
    fn decode(src: &str, toks: &[SemanticToken]) -> Vec<(String, u32, u32)> {
        let line_starts: Vec<usize> = std::iter::once(0)
            .chain(src.match_indices('\n').map(|(i, _)| i + 1))
            .collect();
        let mut out = Vec::new();
        let mut line = 0u32;
        let mut ch = 0u32;
        for t in toks {
            if t.delta_line != 0 {
                line += t.delta_line;
                ch = t.delta_start;
            } else {
                ch += t.delta_start;
            }
            let lo = line_starts[line as usize] + ch as usize;
            let hi = (lo + t.length as usize).min(src.len());
            let text = src.get(lo..hi).unwrap_or("").to_string();
            out.push((text, t.token_type, t.token_modifiers_bitset));
        }
        out
    }

    /// Find the first decoded token whose text equals `name`.
    fn find<'a>(dec: &'a [(String, u32, u32)], name: &str) -> Option<&'a (String, u32, u32)> {
        dec.iter().find(|(t, _, _)| t == name)
    }

    // ── POS: distinct classes for fn / type / var / param / field ────────────────

    #[test]
    fn pos_distinct_token_classes() {
        let src = concat!(
            "module basics.lsp\n",
            "type Point {\n",
            "    ro x int\n",
            "    ro y int\n",
            "}\n",
            "fn add(a int, b int) -> int {\n",
            "    ro sum = a + b + 1\n",
            "    sum\n",
            "}\n",
        );
        let toks = tokens("pos_distinct", src);
        let dec = decode(src, &toks);

        // Function declaration.
        let add = find(&dec, "add").expect("`add` token present");
        assert_eq!(add.1, T_FUNCTION, "fn name -> function");
        assert!(add.2 & M_DECLARATION != 0, "fn name carries declaration modifier");

        // Type declaration.
        let point = find(&dec, "Point").expect("`Point` token present");
        assert_eq!(point.1, T_TYPE, "type name -> type");
        assert!(point.2 & M_DECLARATION != 0, "type name carries declaration modifier");

        // Field.
        let x = find(&dec, "x").expect("`x` field token present");
        assert_eq!(x.1, T_PROPERTY, "record field -> property");
        assert!(x.2 & M_DECLARATION != 0, "field declaration modifier");
        assert!(x.2 & M_READONLY != 0, "ro field -> readonly modifier");

        // Parameter (declaration site).
        let a = find(&dec, "a").expect("`a` param token present");
        assert_eq!(a.1, T_PARAMETER, "param -> parameter");
        assert!(a.2 & M_DECLARATION != 0, "param declaration modifier");

        // Local variable declaration.
        let sum = find(&dec, "sum").expect("`sum` token present");
        assert_eq!(sum.1, T_VARIABLE, "local binding -> variable");
        assert!(sum.2 & M_DECLARATION != 0, "local declaration modifier");

        // Keyword.
        assert!(
            dec.iter().any(|(t, ty, _)| t == "fn" && *ty == T_KEYWORD),
            "`fn` keyword classified as keyword"
        );
        // Number literal.
        assert!(
            dec.iter().any(|(_, ty, _)| *ty == T_NUMBER),
            "at least one numeric literal token"
        );
    }

    // ── POS: parameter USE inside body classifies as parameter, not variable ─────

    #[test]
    fn pos_param_use_is_parameter() {
        let src = concat!(
            "module basics.lsp\n",
            "fn twice(n int) -> int => n + n\n",
        );
        let toks = tokens("pos_param_use", src);
        let dec = decode(src, &toks);
        // Both `n` occurrences (decl + use) classify as parameter.
        let ns: Vec<_> = dec.iter().filter(|(t, _, _)| t == "n").collect();
        assert!(ns.len() >= 2, "expected >=2 `n` tokens (decl + use), got {}", ns.len());
        assert!(ns.iter().all(|(_, ty, _)| *ty == T_PARAMETER), "every `n` -> parameter");
    }

    // ── POS: call callee classified as function ──────────────────────────────────

    #[test]
    fn pos_call_and_member_classes() {
        let src = concat!(
            "module basics.lsp\n",
            "fn helper() -> int => 1\n",
            "fn main() -> () {\n",
            "    ro _ = helper()\n",
            "}\n",
        );
        let toks = tokens("pos_call", src);
        let dec = decode(src, &toks);
        // Two `helper` tokens: declaration (function+declaration) and the call.
        let helpers: Vec<_> = dec.iter().filter(|(t, _, _)| t == "helper").collect();
        assert_eq!(helpers.len(), 2, "decl + call site");
        assert!(helpers.iter().all(|(_, ty, _)| *ty == T_FUNCTION), "both -> function");
    }

    // ── POS/REGRESS: cached @field read keeps the `cached` modifier ──────────────

    const CACHE_SRC: &str = concat!(
        "module semtok.cache\n",
        "type Box {\n",
        "    ro x int\n",
        "    ro y int\n",
        "}\n",
        "fn Box @sum_sq() -> int {\n",
        "    @x * @x + @y * @y\n",
        "}\n",
    );

    #[test]
    fn regress_cached_at_field_modifier() {
        let toks = tokens("regress_cached", CACHE_SRC);
        let dec = decode(CACHE_SRC, &toks);
        // `@x` reads are property tokens with readonly|cached bits set.
        let ats: Vec<_> = dec
            .iter()
            .filter(|(t, ty, _)| t == "@x" && *ty == T_PROPERTY)
            .collect();
        assert!(!ats.is_empty(), "expected @x property tokens, decoded: {dec:?}");
        for (_, _, mods) in &ats {
            assert!(
                mods & M_CACHED != 0 && mods & M_READONLY != 0,
                "cached @field read must carry cached|readonly modifiers, got {mods:#b}"
            );
        }
    }

    // ── POS: delta on a small (tail-append) edit is a single edit ────────────────

    #[test]
    fn pos_delta_single_edit_on_tail_append() {
        let src1 = concat!(
            "module basics.lsp\n",
            "fn main() -> () {\n",
            "    ro a = 1\n",
            "}\n",
        );
        let src2 = concat!(
            "module basics.lsp\n",
            "fn main() -> () {\n",
            "    ro a = 1\n",
            "    ro b = 2\n",
            "}\n",
        );
        let t1 = tokens("delta_v1", src1);
        let t2 = tokens("delta_v2", src2);
        assert!(t2.len() > t1.len(), "adding a binding adds tokens");
        let edits = compute_semantic_token_edits(&t1, &t2);
        assert_eq!(edits.len(), 1, "a tail append yields a single edit, got {edits:?}");
        assert!(edits[0].start > 0, "edit starts after the shared prefix");
    }

    // ── NEG: undeclared identifier is a variable, not a function ─────────────────

    #[test]
    fn neg_undeclared_ident_is_variable() {
        let src = concat!(
            "module basics.lsp\n",
            "fn main() -> () {\n",
            "    ro x = 1\n",
            "    ro y = x\n",
            "}\n",
        );
        let toks = tokens("neg_undeclared", src);
        let dec = decode(src, &toks);
        // The use of `x` in `ro y = x` is a plain variable (not a function/type).
        let x_uses: Vec<_> = dec.iter().filter(|(t, _, _)| t == "x").collect();
        assert!(x_uses.iter().all(|(_, ty, _)| *ty == T_VARIABLE), "`x` -> variable");
    }

    // ── NEG: module without cached @field reads emits no cached modifier ─────────

    #[test]
    fn neg_no_cached_modifier_without_field_reads() {
        let src = concat!(
            "module semtok.nocache\n",
            "fn add(a int, b int) -> int => a + b\n",
        );
        let toks = tokens("neg_nocache", src);
        assert!(
            toks.iter().all(|t| t.token_modifiers_bitset & M_CACHED == 0),
            "no @field reads -> no cached modifier anywhere"
        );
    }

    // ── EDGE: parse error degrades without panic ─────────────────────────────────

    #[test]
    fn edge_parse_error_no_panic() {
        let src = "module basics.lsp\nfn broken(@@@@ =>";
        let path = write_temp("edge_parse_err", src);
        let resolved = resolve_module_for_ide(&path, src);
        // The point is: no panic (lexing may still yield some tokens).
        let _ = compute_semantic_tokens(src, &resolved);
    }

    // ── EDGE: large file completes and produces many tokens ──────────────────────

    #[test]
    fn edge_large_file_perf() {
        let mut src = String::from("module basics.lsp\n");
        for i in 0..400 {
            src.push_str(&format!(
                "fn f{i}(a int, b int) -> int {{\n    ro s = a + b\n    s + {i}\n}}\n"
            ));
        }
        let path = write_temp("edge_large", &src);
        let resolved = resolve_module_for_ide(&path, &src);
        let start = std::time::Instant::now();
        let toks = compute_semantic_tokens(&src, &resolved);
        let elapsed = start.elapsed();
        assert!(toks.len() > 400 * 8, "expected many tokens, got {}", toks.len());
        assert!(
            elapsed.as_secs() < 10,
            "large-file semantic pass should be well under 10s, took {elapsed:?}"
        );
    }

    // ── EDGE: multi-line doc-comment splits into per-line comment tokens ──────────

    #[test]
    fn edge_multiline_doc_comment_split() {
        let src = concat!(
            "module basics.lsp\n",
            "/// line one\n",
            "/// line two\n",
            "fn f() -> () {}\n",
        );
        let toks = tokens("edge_doc", src);
        let comments: Vec<_> = toks.iter().filter(|t| t.token_type == T_COMMENT).collect();
        assert_eq!(comments.len(), 2, "merged /// block splits into two per-line tokens");
        for c in &comments {
            assert!(c.length > 0, "comment token has positive length");
        }
    }
}
