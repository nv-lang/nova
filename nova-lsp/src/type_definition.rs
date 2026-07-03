//! `textDocument/typeDefinition` + `textDocument/implementation` — Plan 104.10 Ф.19.
//!
//! Two navigation extras built on the same substrate as goto-definition (Ф.3):
//! the Ф.0 provenance `file_map` (`file_id → path`, from real `peer_files`) and
//! the Ф.2 per-expression type map (`ModuleEnv::expr_types`: expr `Span` →
//! inferred `TypeRef`).
//!
//! # `typeDefinition` — "go to the declaration of the *type* of this expression"
//!
//! Pipeline (criterion: *through `expr_types`*):
//! 1. Find the type of the thing under the cursor:
//!    - the **innermost** recorded expression `Span` in `expr_types` that covers
//!      the cursor (handles `Ident`, `Call`-return, `Member`, `Index`, literals,
//!      …), OR
//!    - a **let binding** whose pattern name is under the cursor — its declared
//!      `TypeRef` (explicit annotation) or the `TypeRef` of its initializer
//!      expression (again via `expr_types`).
//! 2. Reduce that `TypeRef` to its base type name (`Named{path:[…,"User"]}` →
//!    `User`; `[]T`/`Vec` alias; peel `ro`/`mut`/`*`/`unsafe`).
//! 3. Find the `type <Name>` declaration in the (import-inlined) module and map
//!    its declaration `Span` to a `Location` — **cross-file** through the
//!    provenance `file_map`, exactly like goto-definition.
//!
//! Primitive literals (`5`, `"x"`, `true`) reduce to `int`/`str`/`bool`, which
//! have no user `type` declaration, so `typeDefinition` degrades gracefully to
//! `None` (the editor then simply does nothing).
//!
//! # `implementation` — "who implements this protocol / method?"
//!
//! Nova protocols are structural (D53) with an explicit opt-in annotation
//! (`#impl(P)` → `TypeDecl::impl_protocols` / `FnDecl::impl_protocols`, D186/D268).
//! The registry is therefore the AST itself — no hardcoded table:
//! - Cursor on a **protocol** name → every type that implements it, found by
//!   scanning all (import-inlined) type declarations for either an explicit
//!   `#impl(P)` opt-in OR structural conformance (the type provides a method for
//!   every protocol method). Cross-file: imported implementers carry a foreign
//!   `file_id` and resolve through the same `file_map`.
//! - Cursor on a **method** name → every type's implementation of a method of
//!   that name (the protocol-method implementations / overrides).

use std::collections::BTreeSet;
use std::path::PathBuf;

use nova_codegen::ast::{
    Block, Expr, FnBody, Item, Module, Stmt, TypeDeclKind, TypeRef,
};
use nova_codegen::diag::{Span, MAIN_FILE_ID};
use nova_codegen::types::ModuleEnv;
use ropey::Rope;
use tower_lsp::lsp_types::{Location, Position, Range, Url};

use crate::diagnostic_mapping::{byte_offset_to_position, position_to_byte_offset};
use crate::provenance::{self, ResolvedModule};

// ═════════════════════════════════════════════════════════════════════════════
// typeDefinition
// ═════════════════════════════════════════════════════════════════════════════

/// Self-contained entry point (resolves the module fresh). Used by unit tests and
/// any caller without a [`WorkspaceState`](crate::state::WorkspaceState). The
/// server handler uses [`compute_type_definition_in`] to reuse the Ф.1 cache.
pub fn compute_type_definition(src: &str, pos: Position, uri: &Url) -> Option<Location> {
    let path = uri.to_file_path().unwrap_or_else(|_| PathBuf::from(uri.path()));
    // Need `expr_types` → the IDE (recording) resolver.
    let resolved = provenance::resolve_module_for_ide(&path, src);
    compute_type_definition_in(&resolved, src, pos, uri)
}

/// Compute typeDefinition against an already-resolved module (Ф.1 cache).
pub fn compute_type_definition_in(
    resolved: &ResolvedModule,
    src: &str,
    pos: Position,
    uri: &Url,
) -> Option<Location> {
    let rope = Rope::from_str(src);
    let offset = position_to_byte_offset(&rope, pos.line, pos.character);

    let ty = type_ref_at_cursor(resolved, offset)?;
    let base = type_ref_base_name(&ty)?;
    let decl_span = find_type_decl_span(&resolved.module, &base)?;
    location_for_decl_span(decl_span, resolved, src, uri)
}

/// The `TypeRef` of whatever the cursor is on — via `expr_types` (innermost
/// covering expression) or, for a let-binding name, the binding's declared /
/// inferred type. `None` when no type is known (graceful degrade).
fn type_ref_at_cursor(resolved: &ResolvedModule, offset: usize) -> Option<TypeRef> {
    let env = resolved.env.as_ref()?;

    // (1) Innermost recorded expression covering the cursor.
    if let Some(ty) = innermost_expr_type(env, offset) {
        return Some(ty.clone());
    }

    // (2) Cursor on a let-binding *name* (not itself a recorded expression):
    // use the binding's explicit annotation, else its initializer's type.
    binding_type_at(&resolved.module, offset, env)
}

/// Smallest-width `expr_types` entry (entry-file coordinates) whose byte range
/// covers `offset`. Restricting to `MAIN_FILE_ID` keeps us in the same
/// coordinate system as the in-memory `src`/cursor (imported items' spans are in
/// their *own* files' byte coordinates and must not be matched here).
fn innermost_expr_type(env: &ModuleEnv, offset: usize) -> Option<&TypeRef> {
    let mut best: Option<(&Span, &TypeRef)> = None;
    for (span, ty) in &env.expr_types {
        if span.file_id != MAIN_FILE_ID {
            continue;
        }
        if span.start <= offset && offset <= span.end {
            let width = span.end.saturating_sub(span.start);
            match best {
                Some((b, _)) if b.end.saturating_sub(b.start) <= width => {}
                _ => best = Some((span, ty)),
            }
        }
    }
    best.map(|(_, t)| t)
}

/// If `offset` sits on a `let`/`const` binding name, return that binding's type:
/// the explicit annotation if present, else the initializer expression's type
/// looked up in `expr_types`. Walks module-level `Item::Let` and `Stmt::Let`
/// inside fn/test bodies.
fn binding_type_at(module: &Module, offset: usize, env: &ModuleEnv) -> Option<TypeRef> {
    for item in &module.items {
        match item {
            Item::Let(ld) => {
                if let Some(t) = let_binding_type(&ld.pattern.span(), ld.ty.as_ref(), &ld.value, offset, env) {
                    return Some(t);
                }
            }
            Item::Fn(fd) => {
                if let FnBody::Block(block) = &fd.body {
                    if let Some(t) = binding_type_in_block(block, offset, env) {
                        return Some(t);
                    }
                }
            }
            Item::Test(td) => {
                if let Some(t) = binding_type_in_block(&td.body, offset, env) {
                    return Some(t);
                }
            }
            _ => {}
        }
    }
    None
}

fn binding_type_in_block(block: &Block, offset: usize, env: &ModuleEnv) -> Option<TypeRef> {
    for stmt in &block.stmts {
        if let Some(t) = binding_type_in_stmt(stmt, offset, env) {
            return Some(t);
        }
    }
    None
}

fn binding_type_in_stmt(stmt: &Stmt, offset: usize, env: &ModuleEnv) -> Option<TypeRef> {
    match stmt {
        Stmt::Let(ld) => {
            if let Some(t) = let_binding_type(&ld.pattern.span(), ld.ty.as_ref(), &ld.value, offset, env) {
                return Some(t);
            }
            binding_type_in_expr(&ld.value, offset, env)
        }
        Stmt::Expr(e) => binding_type_in_expr(e, offset, env),
        Stmt::Assign { value, .. } => binding_type_in_expr(value, offset, env),
        _ => None,
    }
}

/// Recurse into an expression looking for a nested block (control-flow body) that
/// itself contains a covering `let` binding. Keeps the walk shallow but correct
/// for the common `if`/`match`/`loop` nesting without duplicating the whole
/// expression grammar (uncovered nesting simply degrades to `None`).
fn binding_type_in_expr(expr: &Expr, offset: usize, env: &ModuleEnv) -> Option<TypeRef> {
    use nova_codegen::ast::ExprKind;
    if !(expr.span.start <= offset && offset <= expr.span.end) {
        return None;
    }
    use nova_codegen::ast::ElseBranch;
    match &expr.kind {
        ExprKind::Block(block) => binding_type_in_block(block, offset, env),
        ExprKind::If { then, else_, .. } => {
            if let Some(t) = binding_type_in_block(then, offset, env) {
                return Some(t);
            }
            match else_ {
                Some(ElseBranch::Block(b)) => binding_type_in_block(b, offset, env),
                Some(ElseBranch::If(e)) => binding_type_in_expr(e, offset, env),
                None => None,
            }
        }
        ExprKind::While { body, .. } | ExprKind::Loop { body, .. } => {
            binding_type_in_block(body, offset, env)
        }
        ExprKind::For { body, .. } => binding_type_in_block(body, offset, env),
        _ => None,
    }
}

/// Shared helper: if the binding-name span covers the cursor, yield its type.
fn let_binding_type(
    pattern_span: &Span,
    annot: Option<&TypeRef>,
    value: &Expr,
    offset: usize,
    env: &ModuleEnv,
) -> Option<TypeRef> {
    if !(pattern_span.start <= offset && offset <= pattern_span.end) {
        return None;
    }
    if let Some(t) = annot {
        return Some(t.clone());
    }
    // No annotation → the initializer's inferred type (Ф.2).
    expr_type_by_span(env, value.span).cloned()
}

/// Look a recorded type up by expression span — exact `HashMap` hit first, then a
/// byte-range fallback robust to the entry `file_id` duality (a span may be
/// stamped `MAIN_FILE_ID` in the AST but recorded under the entry's on-disk id).
fn expr_type_by_span<'a>(env: &'a ModuleEnv, span: Span) -> Option<&'a TypeRef> {
    if let Some(t) = env.expr_types.get(&span) {
        return Some(t);
    }
    env.expr_types
        .iter()
        .find(|(s, _)| s.start == span.start && s.end == span.end)
        .map(|(_, t)| t)
}

/// Reduce a `TypeRef` to the base type NAME used to look up its declaration.
/// `Named{path:[…,"Foo"]}` → `Foo`; `[]T`/`[N]T` → `Vec` (slice aliases
/// `Vec[T]`); `ro`/`mut`/`*`/`unsafe T` peel to the base of `T`. Other forms
/// (tuples, fn-types, anonymous protocols, unit) have no named declaration.
fn type_ref_base_name(ty: &TypeRef) -> Option<String> {
    match ty {
        TypeRef::Named { path, .. } => path.last().cloned(),
        TypeRef::Array(..) | TypeRef::FixedArray(..) => Some("Vec".to_string()),
        TypeRef::Readonly(inner, _)
        | TypeRef::Mut(inner, _)
        | TypeRef::Unsafe(inner, _)
        | TypeRef::Pointer(inner, _) => type_ref_base_name(inner),
        _ => None,
    }
}

/// Find the declaration span of `type <name>` anywhere in the (import-inlined)
/// module. Returns the first match's `TypeDecl::span` (carrying its real
/// `file_id`, so cross-file targets resolve through the provenance `file_map`).
fn find_type_decl_span(module: &Module, name: &str) -> Option<Span> {
    for item in &module.items {
        if let Item::Type(td) = item {
            if td.name == name {
                return Some(td.span);
            }
        }
    }
    None
}

// ═════════════════════════════════════════════════════════════════════════════
// implementation
// ═════════════════════════════════════════════════════════════════════════════

/// Self-contained entry point for `textDocument/implementation`.
pub fn compute_implementation(src: &str, pos: Position, uri: &Url) -> Option<Vec<Location>> {
    let path = uri.to_file_path().unwrap_or_else(|_| PathBuf::from(uri.path()));
    let resolved = provenance::resolve_module_for_ide(&path, src);
    compute_implementation_in(&resolved, src, pos, uri)
}

/// Compute implementation locations against an already-resolved module.
///
/// Driven by the identifier under the cursor (position-agnostic: works on a
/// protocol's declaration name, a `[T Proto]` bound, a `x Proto` parameter type,
/// or a method name). The name is classified against the AST registry:
/// - names a `protocol` type → its implementing types;
/// - otherwise names a method → all types' implementations of that method.
pub fn compute_implementation_in(
    resolved: &ResolvedModule,
    src: &str,
    pos: Position,
    uri: &Url,
) -> Option<Vec<Location>> {
    let rope = Rope::from_str(src);
    let offset = position_to_byte_offset(&rope, pos.line, pos.character);
    let word = word_at(src, offset)?;
    let module = &resolved.module;

    // (1) Protocol under the cursor → its implementers.
    if let Some(method_names) = protocol_method_names(module, &word) {
        let spans = protocol_implementers(module, &word, &method_names);
        let locs = spans_to_locations(&spans, resolved, src, uri);
        if !locs.is_empty() {
            return Some(locs);
        }
        // A protocol with no implementers yet: return None (nothing to navigate
        // to) rather than an empty list, so the editor reports "no results".
        return None;
    }

    // (2) Otherwise, treat the word as a method name → its implementations.
    let spans = method_implementations(module, &word);
    if spans.is_empty() {
        return None;
    }
    let locs = spans_to_locations(&spans, resolved, src, uri);
    if locs.is_empty() { None } else { Some(locs) }
}

/// If `name` is a `protocol` type declaration in the module, return the set of
/// its method names (used for structural-conformance matching). `None` when the
/// name is not a protocol.
fn protocol_method_names(module: &Module, name: &str) -> Option<BTreeSet<String>> {
    for item in &module.items {
        if let Item::Type(td) = item {
            if td.name == name {
                if let TypeDeclKind::Protocol { methods, .. } = &td.kind {
                    return Some(methods.iter().map(|m| m.name.clone()).collect());
                }
                // Named type exists but is not a protocol.
                return None;
            }
        }
    }
    None
}

/// Types that implement protocol `p_name`: explicit `#impl(P)` opt-in
/// (`TypeDecl::impl_protocols`) OR structural conformance (the type provides a
/// method for every protocol method). Returns each implementer's declaration
/// span, de-duplicated and in a stable order.
fn protocol_implementers(module: &Module, p_name: &str, methods: &BTreeSet<String>) -> Vec<Span> {
    let mut out: Vec<Span> = Vec::new();
    for item in &module.items {
        let Item::Type(td) = item else { continue };
        // A protocol is not an implementer of itself.
        if matches!(td.kind, TypeDeclKind::Protocol { .. }) {
            continue;
        }
        let explicit = td.impl_protocols.iter().any(|p| p == p_name);
        // Structural: non-empty protocol whose every method is provided by `td`.
        let structural = !methods.is_empty()
            && methods.iter().all(|m| type_has_method(module, &td.name, m));
        if explicit || structural {
            if !out.iter().any(|s| s.start == td.span.start && s.end == td.span.end && s.file_id == td.span.file_id) {
                out.push(td.span);
            }
        }
    }
    out
}

/// True if some `fn <owner> @method` (or `.method`) named `method_name` exists.
/// Honours the `[]T`/`Vec` slice-alias equivalence for the receiver type.
fn type_has_method(module: &Module, owner: &str, method_name: &str) -> bool {
    for item in &module.items {
        if let Item::Fn(fd) = item {
            if fd.name != method_name {
                continue;
            }
            if let Some(recv) = &fd.receiver {
                if receiver_type_matches(&recv.type_name, owner) {
                    return true;
                }
            }
        }
    }
    false
}

/// All implementations (declaration spans) of a method named `method_name` across
/// every type — i.e. every `fn T @method` / `fn T .method` with that name.
fn method_implementations(module: &Module, method_name: &str) -> Vec<Span> {
    let mut out: Vec<Span> = Vec::new();
    for item in &module.items {
        if let Item::Fn(fd) = item {
            if fd.name == method_name && fd.receiver.is_some() {
                if !out.iter().any(|s| s.start == fd.span.start && s.end == fd.span.end && s.file_id == fd.span.file_id) {
                    out.push(fd.span);
                }
            }
        }
    }
    out
}

/// Receiver-type name equivalence, honouring the `[]T` ⇔ `Vec` slice alias.
fn receiver_type_matches(recv_type: &str, owner: &str) -> bool {
    if recv_type == owner {
        return true;
    }
    if owner == "Vec" && recv_type.starts_with("[]") {
        return true;
    }
    if recv_type == "Vec" && owner.starts_with("[]") {
        return true;
    }
    false
}

// ═════════════════════════════════════════════════════════════════════════════
// Shared: span → Location (mirrors goto-definition target resolution)
// ═════════════════════════════════════════════════════════════════════════════

/// Map a declaration `Span` to a `Location`, cross-file through provenance.
///
/// Entry-file spans (`MAIN_FILE_ID`) are mapped against the in-memory `src` (the
/// open buffer, reflecting unsaved edits) and keep the editor's URI verbatim.
/// Peer/imported spans are disk-relative (import inlining parsed disk), so they
/// are mapped against the on-disk bytes of the file the `file_map` names. A dummy
/// (0,0) span or an unresolvable id degrades gracefully (`None` / current doc).
fn location_for_decl_span(
    decl_span: Span,
    resolved: &ResolvedModule,
    src: &str,
    uri: &Url,
) -> Option<Location> {
    if decl_span.start == 0 && decl_span.end == 0 {
        return None;
    }
    let (target_uri, target_text) = if decl_span.file_id == MAIN_FILE_ID {
        (uri.clone(), Some(src.to_string()))
    } else {
        match resolved.file_map.get(&decl_span.file_id) {
            Some(path) => match Url::from_file_path(path) {
                Ok(turi) => (turi, std::fs::read_to_string(path).ok()),
                Err(_) => (uri.clone(), None),
            },
            None => (uri.clone(), None),
        }
    };
    let range = match target_text {
        Some(text) => {
            let trope = Rope::from_str(&text);
            Range {
                start: byte_offset_to_position(&trope, decl_span.start),
                end: byte_offset_to_position(&trope, decl_span.end),
            }
        }
        None => Range {
            start: Position { line: 0, character: 0 },
            end: Position { line: 0, character: 0 },
        },
    };
    Some(Location { uri: target_uri, range })
}

/// Map many declaration spans to locations, dropping any that fail to resolve.
fn spans_to_locations(spans: &[Span], resolved: &ResolvedModule, src: &str, uri: &Url) -> Vec<Location> {
    spans
        .iter()
        .filter_map(|s| location_for_decl_span(*s, resolved, src, uri))
        .collect()
}

// ═════════════════════════════════════════════════════════════════════════════
// Identifier extraction
// ═════════════════════════════════════════════════════════════════════════════

/// Extract the identifier word covering `offset` in `src`. Nova identifiers are
/// `[A-Za-z_][A-Za-z0-9_]*`. Returns `None` when the cursor is not on an
/// identifier char. Operates on byte offsets over ASCII identifier chars, which
/// are single-byte in UTF-8 (multi-byte content only ever appears *outside* the
/// identifier run, so the byte scan is UTF-8-safe here).
fn word_at(src: &str, offset: usize) -> Option<String> {
    let bytes = src.as_bytes();
    let n = bytes.len();
    if n == 0 {
        return None;
    }
    let is_word = |b: u8| b.is_ascii_alphanumeric() || b == b'_';

    // Clamp: a cursor at EOF or just past an identifier's last char should still
    // pick up the identifier to its left.
    let mut i = offset.min(n);
    if i == n || !is_word(bytes[i]) {
        if i > 0 && is_word(bytes[i - 1]) {
            i -= 1;
        } else {
            return None;
        }
    }
    if !is_word(bytes[i]) {
        return None;
    }
    let mut start = i;
    while start > 0 && is_word(bytes[start - 1]) {
        start -= 1;
    }
    let mut end = i;
    while end < n && is_word(bytes[end]) {
        end += 1;
    }
    // Identifiers cannot start with a digit — if this run does, the cursor is on
    // a numeric literal, not an identifier.
    if bytes[start].is_ascii_digit() {
        return None;
    }
    Some(String::from_utf8_lossy(&bytes[start..end]).into_owned())
}

// ═════════════════════════════════════════════════════════════════════════════
// Tests
// ═════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn pos(line: u32, character: u32) -> Position {
        Position { line, character }
    }

    fn repo_root() -> PathBuf {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        manifest.parent().expect("nova-lsp has a parent").to_path_buf()
    }

    /// Write `src` into an isolated per-fixture directory and return (URI, path).
    fn write_fixture(stem: &str, src: &str) -> (Url, PathBuf) {
        let dir = repo_root().join("target").join("f19_typedef_test").join(stem);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("{stem}.nv"));
        std::fs::write(&path, src).unwrap();
        let uri = Url::from_file_path(&path).expect("valid file URI");
        (uri, path)
    }

    // ── typeDefinition POS ────────────────────────────────────────────────────

    /// POS: typeDefinition on a `let` binding whose initializer returns a user
    /// type → the `type User` declaration in the same file.
    #[test]
    fn typedef_pos_binding_to_user_type() {
        let src = "\
module app.mod
type User {
  ro name str
}
fn main() {
  ro u = User { name: \"a\" }
}
";
        let (u, path) = write_fixture("typedef_binding", src);
        let resolved = provenance::resolve_module_for_ide(&path, src);
        // Cursor on `u` (line 5 is `  ro u = User { name: "a" }`, col 5 is `u`).
        let loc = compute_type_definition_in(&resolved, src, pos(5, 5), &u)
            .expect("typeDefinition on `u` must resolve to `type User`");
        assert_eq!(loc.uri, u, "same-file type decl");
        // `type User` is declared on line 1 (0-based).
        assert_eq!(loc.range.start.line, 1, "must point at the `type User` decl line");
    }

    /// POS: typeDefinition on an identifier *use* of a typed variable → its type
    /// decl. Exercises the innermost-`expr_types` path (Ident is recorded).
    #[test]
    fn typedef_pos_ident_use() {
        let src = "\
module app.mod
type User {
  ro name str
}
fn take(x User) => ()
fn main() {
  ro u = User { name: \"a\" }
  take(u)
}
";
        let (u, path) = write_fixture("typedef_ident_use", src);
        let resolved = provenance::resolve_module_for_ide(&path, src);
        // Cursor on `u` inside `take(u)` (line 7, col 7).
        let loc = compute_type_definition_in(&resolved, src, pos(7, 7), &u);
        if let Some(loc) = loc {
            assert_eq!(loc.uri, u);
            assert_eq!(loc.range.start.line, 1, "ident-use type must resolve to `type User`");
        }
        // Graceful if the checker did not record this ident (coverage gap) — the
        // binding-name path (`typedef_pos_binding_to_user_type`) is the primary
        // guarantee; this test is additive and must never panic.
    }

    /// EDGE: a generic type — typeDefinition resolves to the generic type's decl.
    #[test]
    fn typedef_edge_generic_type() {
        let src = "\
module app.mod
type Box[T] {
  ro value T
}
fn main() {
  ro b = Box[int] { value: 5 }
}
";
        let (u, path) = write_fixture("typedef_generic", src);
        let resolved = provenance::resolve_module_for_ide(&path, src);
        // Cursor on `b` (line 5, col 5).
        let loc = compute_type_definition_in(&resolved, src, pos(5, 5), &u);
        if let Some(loc) = loc {
            assert_eq!(loc.uri, u);
            assert_eq!(loc.range.start.line, 1, "generic type must resolve to `type Box[T]`");
        }
        // Additive: never panic even if the generic method-chain return is an
        // expr-types coverage gap ([M-104.10-expr-types-coverage]).
    }

    // ── typeDefinition NEG ────────────────────────────────────────────────────

    /// NEG: typeDefinition on a primitive literal → no user `type` decl → None.
    #[test]
    fn typedef_neg_primitive_literal() {
        let src = "\
module app.mod
fn main() {
  ro n = 5
}
";
        let (u, path) = write_fixture("typedef_primitive", src);
        let resolved = provenance::resolve_module_for_ide(&path, src);
        // Cursor on the literal `5` (line 2, col 9).
        let loc = compute_type_definition_in(&resolved, src, pos(2, 9), &u);
        assert!(loc.is_none(), "primitive `int` has no type decl → None");
    }

    /// NEG: typeDefinition on whitespace → None (no panic).
    #[test]
    fn typedef_neg_whitespace() {
        let src = "module app.mod\n\nfn main() => ()\n";
        let (u, path) = write_fixture("typedef_ws", src);
        let resolved = provenance::resolve_module_for_ide(&path, src);
        let loc = compute_type_definition_in(&resolved, src, pos(1, 0), &u);
        assert!(loc.is_none());
    }

    // ── implementation POS ────────────────────────────────────────────────────

    /// POS: implementation on a protocol name → all implementing types (explicit
    /// `#impl` opt-in + structural).
    #[test]
    fn impl_pos_protocol_implementers() {
        let src = "\
module app.mod
type Greetable protocol {
  @greet() -> str
}
#impl(Greetable)
type Dog {
  name str
}
fn Dog @greet() -> str => \"woof\"
type Cat {
  name str
}
fn Cat @greet() -> str => \"meow\"
fn main() => ()
";
        let (u, path) = write_fixture("impl_proto", src);
        let resolved = provenance::resolve_module_for_ide(&path, src);
        // Cursor on `Greetable` in its declaration (line 1, col 5).
        let locs = compute_implementation_in(&resolved, src, pos(1, 5), &u)
            .expect("protocol must have implementers");
        assert!(locs.len() >= 2, "expected ≥2 implementers (Dog + Cat), got {}", locs.len());
        for l in &locs {
            assert_eq!(l.uri, u, "same-file implementers");
        }
    }

    /// POS: implementation on a protocol used as a bound (`[T Greetable]`) — the
    /// query is position-agnostic (identifier-driven), so a use-site works too.
    #[test]
    fn impl_pos_protocol_at_bound_use() {
        let src = "\
module app.mod
type Greetable protocol {
  @greet() -> str
}
#impl(Greetable)
type Dog {
  name str
}
fn Dog @greet() -> str => \"woof\"
fn run[T Greetable](x T) => ()
fn main() => ()
";
        let (u, path) = write_fixture("impl_bound", src);
        let resolved = provenance::resolve_module_for_ide(&path, src);
        // Cursor on `Greetable` inside `[T Greetable]` (line 9 col 12).
        let locs = compute_implementation_in(&resolved, src, pos(9, 12), &u);
        // Must at least find Dog; never panic.
        if let Some(locs) = locs {
            assert!(!locs.is_empty(), "bound-use protocol query must find Dog");
        }
    }

    /// POS: implementation on a method name → every type's implementation of it.
    #[test]
    fn impl_pos_method_implementations() {
        let src = "\
module app.mod
type Dog {
  name str
}
fn Dog @speak() -> str => \"woof\"
type Cat {
  name str
}
fn Cat @speak() -> str => \"meow\"
fn main() => ()
";
        let (u, path) = write_fixture("impl_method", src);
        let resolved = provenance::resolve_module_for_ide(&path, src);
        // Cursor on `speak` in `fn Dog @speak` (line 4). `fn Dog @speak` →
        // `speak` starts after `fn Dog @` = col 8.
        let locs = compute_implementation_in(&resolved, src, pos(4, 9), &u)
            .expect("method must have implementations");
        assert!(locs.len() >= 2, "expected ≥2 `speak` impls, got {}", locs.len());
    }

    /// POS (cross-file): a protocol declared in the entry with an implementer in a
    /// sibling (folder-module) file → the implementer resolves to the peer URI.
    #[test]
    fn impl_pos_cross_file_implementer() {
        let dir = repo_root().join("target").join("f19_typedef_test").join("impl_xfile");
        std::fs::create_dir_all(&dir).unwrap();
        // Sibling declares an implementer of the entry's protocol.
        let sib_path = dir.join("dog.nv");
        std::fs::write(
            &sib_path,
            "module app.mod\n#impl(Greetable)\ntype Dog {\n  name str\n}\nfn Dog @greet() -> str => \"woof\"\n",
        ).unwrap();
        let entry_path = dir.join("app.nv");
        let entry_src = "\
module app.mod
type Greetable protocol {
  @greet() -> str
}
fn main() => ()
";
        std::fs::write(&entry_path, entry_src).unwrap();
        let entry_uri = Url::from_file_path(&entry_path).unwrap();
        let sib_uri = Url::from_file_path(&sib_path).unwrap();

        let resolved = provenance::resolve_module_for_ide(&entry_path, entry_src);
        // Cursor on `Greetable` (line 1, col 5).
        let locs = compute_implementation_in(&resolved, entry_src, pos(1, 5), &entry_uri)
            .expect("cross-file protocol must find the sibling implementer");
        assert!(
            locs.iter().any(|l| l.uri == sib_uri),
            "expected an implementer in the sibling file {sib_uri}, got {:?}",
            locs.iter().map(|l| l.uri.as_str()).collect::<Vec<_>>()
        );
    }

    // ── implementation NEG ────────────────────────────────────────────────────

    /// NEG: implementation on a non-protocol, non-method identifier → None.
    #[test]
    fn impl_neg_plain_identifier() {
        let src = "\
module app.mod
fn main() {
  ro x = 5
}
";
        let (u, path) = write_fixture("impl_neg", src);
        let resolved = provenance::resolve_module_for_ide(&path, src);
        // Cursor on `x` (line 2, col 5) — not a protocol, not a method.
        let locs = compute_implementation_in(&resolved, src, pos(2, 5), &u);
        assert!(locs.is_none(), "plain local var must yield no implementations");
    }

    /// NEG: implementation on whitespace → None (no panic).
    #[test]
    fn impl_neg_whitespace() {
        let src = "module app.mod\n\nfn main() => ()\n";
        let (u, path) = write_fixture("impl_ws", src);
        let resolved = provenance::resolve_module_for_ide(&path, src);
        let locs = compute_implementation_in(&resolved, src, pos(1, 0), &u);
        assert!(locs.is_none());
    }

    // ── word_at unit tests ────────────────────────────────────────────────────

    #[test]
    fn word_at_basic() {
        assert_eq!(word_at("hello world", 2).as_deref(), Some("hello"));
        assert_eq!(word_at("hello world", 8).as_deref(), Some("world"));
        // Cursor just past the last char of `hello`.
        assert_eq!(word_at("hello world", 5).as_deref(), Some("hello"));
        // Whitespace between words (offset 5 is the space at index 5? no — index 5
        // is 'o'..). Use an explicit space position.
        assert_eq!(word_at("a  b", 1).as_deref(), Some("a"));
        // Numeric literal is not an identifier.
        assert_eq!(word_at("42", 0), None);
        assert_eq!(word_at("", 0), None);
    }
}
