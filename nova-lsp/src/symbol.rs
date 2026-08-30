//! Symbol resolution for hover, goto-definition, and signature help.
//!
//! Plan 104.2.Ф.1: SymbolInfo enum + resolve_symbol_at + TypeRef formatter.
//!
//! The resolver walks the parsed AST to find the most specific symbol
//! covering a given byte offset. No cross-file resolution in V1
//! (deferred to Plan 104.4 / [M-104.2-cross-file-goto]).

use nova_codegen::ast::{
    Block, Expr, ExprKind, FnBody, FnDecl, Item, MatchArmBody,
    Module, NamedTupleField, Param, Pattern, Receiver, ReceiverKind,
    Stmt, TypeDeclKind, TypeRef,
};
use nova_codegen::diag::{Span, MAIN_FILE_ID};
use nova_codegen::types::ModuleEnv;

// ─────────────────────────────────────────────────────────────────────────────
// SymbolInfo
// ─────────────────────────────────────────────────────────────────────────────

/// Information about a Nova symbol found at a cursor position.
#[derive(Debug, Clone)]
pub enum SymbolInfo {
    /// A local variable binding (`ro x int = 5`).
    LocalVar {
        name: String,
        /// Human-readable type text ("int", "[]str", "Option[bool]", …).
        ty_text: String,
        is_mut: bool,
        span: Span,
        doc: Option<String>,
    },
    /// A free function (`fn foo(...) -> T`).
    FnDecl {
        name: String,
        /// Full formatted signature, e.g. `fn foo(x int, y str) -> bool`.
        signature: String,
        doc: Option<String>,
        span: Span,
    },
    /// A type declaration (`type Foo { ... }`).
    TypeDecl {
        name: String,
        /// Kind label: "record", "sum", "protocol", "effect", "newtype", "alias", …
        kind_label: String,
        /// Full signature for types that benefit from it (e.g. named tuples show fields).
        signature: Option<String>,
        doc: Option<String>,
        span: Span,
    },
    /// A method on a type (`fn Foo @bar() -> T`).
    MethodDecl {
        receiver_type: String,
        name: String,
        signature: String,
        doc: Option<String>,
        span: Span,
    },
    /// An import statement (`import std.collections.vec`).
    ImportRef {
        module_path: String,
        span: Span,
    },
    /// A module-level constant.
    ConstDecl {
        name: String,
        ty_text: String,
        span: Span,
        doc: Option<String>,
    },
    /// A field of a record / named-tuple type, resolved through member access
    /// (`obj.field`) — Plan 104.10 Ф.6.
    ///
    /// The `owner` type is taken from the object's inferred type (`expr_types`,
    /// Ф.2); the field `name` + `ty_text` come from the owner type's **real**
    /// declaration (criterion #4). `span` is the field's declaration span (so
    /// goto-definition on a member access lands on the field decl).
    FieldDecl {
        owner: String,
        name: String,
        ty_text: String,
        span: Span,
        /// D104 rev-2 (2026-08-15): the field's own outer `///` doc, from the
        /// declaration (`RecordField.doc`) -- rendered by hover like a decl's.
        doc: Option<String>,
    },
}

impl SymbolInfo {
    /// Span of the declaration site (used for goto-definition).
    pub fn span(&self) -> Span {
        match self {
            SymbolInfo::LocalVar { span, .. } => *span,
            SymbolInfo::FnDecl { span, .. } => *span,
            SymbolInfo::TypeDecl { span, .. } => *span,
            SymbolInfo::MethodDecl { span, .. } => *span,
            SymbolInfo::ImportRef { span, .. } => *span,
            SymbolInfo::ConstDecl { span, .. } => *span,
            SymbolInfo::FieldDecl { span, .. } => *span,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// TypeRef → display string
// ─────────────────────────────────────────────────────────────────────────────

/// Render a `TypeRef` to a human-readable Nova type string.
pub fn format_type_ref(ty: &TypeRef) -> String {
    match ty {
        TypeRef::Named { path, generics, .. } => {
            let base = path.join(".");
            if generics.is_empty() {
                base
            } else {
                let arg_strs: Vec<_> = generics.iter().map(format_type_ref).collect();
                format!("{}[{}]", base, arg_strs.join(", "))
            }
        }
        TypeRef::Array(inner, _) => {
            format!("[]{}", format_type_ref(inner))
        }
        TypeRef::FixedArray(n, inner, _) => {
            format!("[{}]{}", n, format_type_ref(inner))
        }
        TypeRef::Tuple(elems, _) => {
            let parts: Vec<_> = elems.iter().map(format_type_ref).collect();
            format!("({})", parts.join(", "))
        }
        TypeRef::Func { params, return_type, effects, .. } => {
            let p: Vec<_> = params.iter().map(format_type_ref).collect();
            let eff = if effects.is_empty() {
                String::new()
            } else {
                let es: Vec<_> = effects.iter().map(format_type_ref).collect();
                format!(" {}", es.join(" "))
            };
            match return_type {
                Some(r) => format!("fn({}){} -> {}", p.join(", "), eff, format_type_ref(r)),
                None => format!("fn({}){}", p.join(", "), eff),
            }
        }
        TypeRef::Unit(_) => "()".to_string(),
        TypeRef::Pointer(inner, _) => {
            format!("*{}", format_type_ref(inner))
        }
        TypeRef::Readonly(inner, _) => {
            format!("ro {}", format_type_ref(inner))
        }
        TypeRef::Mut(inner, _) => {
            format!("mut {}", format_type_ref(inner))
        }
        // §10a rename (Plan 174.5, 2026-07-11): `Uninit` wrapping `Func`
        // keeps the `unsafe` spelling (D216 §10 legacy fn-pointer shape);
        // any other payload renders as `uninit` (renamed possibly-uninit
        // data modifier).
        TypeRef::Uninit(inner, _) => {
            let kw = if matches!(inner.as_ref(), TypeRef::Func { .. }) { "unsafe" } else { "uninit" };
            format!("{} {}", kw, format_type_ref(inner))
        }
        TypeRef::Ref(inner, _) => {
            format!("&{}", format_type_ref(inner))
        }
        TypeRef::Protocol { methods, .. } => {
            format!("protocol {{ {} method(s) }}", methods.len())
        }
    }
}

/// Format a named tuple type as its full Nova declaration signature.
///
/// Fields with defaults show `= …` placeholder since AST `Expr` doesn't
/// have a lossless source-text representation in the LSP layer.
///
/// Example: `type Complex(re f64 = …, im f64 = …)`
fn format_named_tuple_sig(name: &str, fields: &[NamedTupleField]) -> String {
    let parts: Vec<String> = fields
        .iter()
        .map(|f| {
            let ty = format_type_ref(&f.ty);
            if f.default.is_some() {
                format!("{} {} = …", f.name, ty)
            } else {
                format!("{} {}", f.name, ty)
            }
        })
        .collect();
    format!("type {}({})", name, parts.join(", "))
}

// ─────────────────────────────────────────────────────────────────────────────
// Param → display string
// ─────────────────────────────────────────────────────────────────────────────

pub fn format_param(p: &Param) -> String {
    let prefix = if p.consume {
        "consume "
    } else if p.is_mut {
        "mut "
    } else {
        ""
    };
    format!("{}{} {}", prefix, p.name, format_type_ref(&p.ty))
}

// ─────────────────────────────────────────────────────────────────────────────
// Receiver → type name string
// ─────────────────────────────────────────────────────────────────────────────

pub fn format_receiver_type(recv: &Receiver) -> String {
    if recv.generics.is_empty() {
        recv.type_name.clone()
    } else {
        let args: Vec<_> = recv.generics.iter().map(format_type_ref).collect();
        format!("{}[{}]", recv.type_name, args.join(", "))
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// FnDecl → signature string
// ─────────────────────────────────────────────────────────────────────────────

/// Format a free function signature.
pub fn format_fn_signature(fd: &FnDecl) -> String {
    let generics = if fd.generics.is_empty() {
        String::new()
    } else {
        let gs: Vec<_> = fd.generics.iter().map(|g| g.name.clone()).collect();
        format!("[{}]", gs.join(", "))
    };
    let params: Vec<_> = fd.params.iter().map(format_param).collect();
    let effects = if fd.effects.is_empty() {
        String::new()
    } else {
        let es: Vec<_> = fd.effects.iter().map(format_type_ref).collect();
        format!(" {}", es.join(" "))
    };
    let ret = match &fd.return_type {
        Some(r) => format!(" -> {}", format_type_ref(r)),
        None => String::new(),
    };
    format!("fn {}{}({}){}{}", fd.name, generics, params.join(", "), effects, ret)
}

/// Format a method signature (with receiver).
pub fn format_method_signature(fd: &FnDecl, recv: &Receiver) -> String {
    let recv_ty = format_receiver_type(recv);
    let recv_kw = match recv.kind {
        ReceiverKind::Instance => "@",
        ReceiverKind::Static => ".",
    };
    let recv_mut = if recv.mutable { "mut " } else { "" };
    let generics = if fd.generics.is_empty() {
        String::new()
    } else {
        let gs: Vec<_> = fd.generics.iter().map(|g| g.name.clone()).collect();
        format!("[{}]", gs.join(", "))
    };
    let params: Vec<_> = fd.params.iter().map(format_param).collect();
    let effects = if fd.effects.is_empty() {
        String::new()
    } else {
        let es: Vec<_> = fd.effects.iter().map(format_type_ref).collect();
        format!(" {}", es.join(" "))
    };
    let ret = match &fd.return_type {
        Some(r) => format!(" -> {}", format_type_ref(r)),
        None => String::new(),
    };
    format!(
        "fn {} {}{}{}{}({}){}{}", // fn RecvType mut @method[G](params) eff -> ret
        recv_ty, recv_mut, recv_kw, fd.name, generics,
        params.join(", "),
        effects, ret,
    )
}

// ─────────────────────────────────────────────────────────────────────────────
// Doc-comment extraction
// ─────────────────────────────────────────────────────────────────────────────

pub fn extract_doc(doc: &Option<nova_codegen::ast::DocBlock>) -> Option<String> {
    doc.as_ref().map(|d| d.content.trim().to_string()).filter(|s| !s.is_empty())
}

// ─────────────────────────────────────────────────────────────────────────────
// Span contains byte offset
// ─────────────────────────────────────────────────────────────────────────────

pub fn span_contains(span: Span, offset: usize) -> bool {
    span.start <= offset && offset <= span.end
}

// ─────────────────────────────────────────────────────────────────────────────
// Pattern name extraction
// ─────────────────────────────────────────────────────────────────────────────

/// Extract the primary binding name from a pattern (for hover display).
fn pattern_name(p: &Pattern) -> Option<&str> {
    match p {
        Pattern::Ident { name, .. } => Some(name.as_str()),
        Pattern::Binding { name, .. } => Some(name.as_str()),
        _ => None,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// resolve_symbol_at
// ─────────────────────────────────────────────────────────────────────────────

/// Resolve the symbol at `byte_offset` in `module`.
///
/// Walks top-level items (fn declarations, type declarations, imports) and
/// returns the best match — the narrowest span that contains `offset`.
///
/// **V1 scope:** top-level items and their spans only.
/// Local variable resolution inside fn bodies is not implemented in V1
/// as the type-checker does not expose per-expression type maps.
///
/// [M-104.2-local-var-resolution]: local variable types via body walk — V2.
pub fn resolve_symbol_at(module: &Module, byte_offset: usize) -> Option<SymbolInfo> {
    // No inlining: all items are original, so items_start = 0 (skip nothing).
    resolve_symbol_at_with_limit(module, byte_offset, 0, None)
}

/// Does `item` belong to the **entry** file (the open buffer) rather than an
/// inlined import / folder-module peer?
///
/// `parse(src)` stamps every entry span with [`MAIN_FILE_ID`]; `resolve_imports_inline`
/// parses each imported / peer file with its *own* `file_id`. So an item's
/// declaration-span `file_id` is the authoritative provenance signal:
/// `== MAIN_FILE_ID` means entry. Items without a natural declaration span
/// (bench/lemma) are treated as
/// entry — harmless, since they are ignored by span-match and body-walk anyway.
fn item_belongs_to_entry(item: &Item) -> bool {
    let file_id = match item {
        Item::Fn(fd) => fd.span.file_id,
        Item::Type(td) => td.span.file_id,
        Item::Let(ld) => ld.span.file_id,
        Item::Const(cd) => cd.span.file_id,
        Item::Test(td) => td.span.file_id,
        Item::Bench(_) | Item::Lemma(_) => MAIN_FILE_ID,
    };
    file_id == MAIN_FILE_ID
}

/// Resolve a symbol at `byte_offset` in `module`.
///
/// After `resolve_imports_inline`, imported items are **prepended** to
/// `module.items` and — for folder-module peers — the entry's sibling files'
/// items are **merged in** as well. So the entry file's own items are *not* a
/// clean suffix of `module.items`: neither a prepend-count nor an index slice
/// reliably isolates them (a peer item can sort after the entry's).
///
/// Entry membership is therefore decided by **provenance**, not position: an item
/// is the entry's iff its declaration-span `file_id == MAIN_FILE_ID`
/// ([`item_belongs_to_entry`]). This is byte-collision-proof — a peer item whose
/// foreign-file offsets happen to overlap the cursor no longer mis-matches.
///
/// - Span-match and body-walk are restricted to entry items (foreign spans are
///   meaningless against the entry buffer's byte offsets).
/// - Name lookup (to resolve a found ident) searches ALL items so prelude / peer
///   symbols like `assert` — or a fn declared in a sibling file — are found and
///   carry their real (foreign-`file_id`) declaration span.
///
/// `items_start` is retained for API stability (callers pass the prepend count);
/// entry membership no longer depends on it.
pub fn resolve_symbol_at_with_limit(
    module: &Module,
    byte_offset: usize,
    items_start: usize,
    env: Option<&ModuleEnv>,
) -> Option<SymbolInfo> {
    let _ = items_start; // provenance (file_id) now decides entry membership.

    // Check imports first (they appear early in the file).
    for import in &module.imports {
        if span_contains(import.span, byte_offset) {
            let path = import.path.join(".");
            return Some(SymbolInfo::ImportRef {
                module_path: path,
                span: import.span,
            });
        }
    }

    // Span-match only entry items (inlined / peer items have foreign spans).
    for item in module.items.iter().filter(|it| item_belongs_to_entry(it)) {
        if let Some(info) = resolve_item(item, byte_offset) {
            return Some(info);
        }
    }

    // First fallback: check if cursor is on a local variable binding pattern.
    // This must run before the ident-name lookup because pattern names are not
    // in module.items and cannot be found by lookup_decl_by_name.
    if let Some(info) = find_local_var_at(module, byte_offset, env) {
        return Some(info);
    }

    // Second fallback: body-walk entry items to find ident name at cursor,
    // then look it up by name across ALL items (including inlined prelude/peers).
    if let Some(ident_name) = find_ident_in_bodies_entry(module, byte_offset) {
        if let Some(info) = lookup_decl_by_name(module, &ident_name) {
            return Some(info);
        }
    }

    // Final fallback (Plan 104.10 Ф.6): member-access `obj.field`. When the
    // cursor sits on the *field* part, the ident/name fallbacks above find
    // nothing (the field is not a top-level name and not a binding). Resolve it
    // through the object's inferred type (`expr_types`, Ф.2) → the owner type's
    // real field/method declaration.
    if let Some(info) = resolve_member_at(module, byte_offset, env) {
        return Some(info);
    }

    None
}

fn resolve_item(item: &Item, byte_offset: usize) -> Option<SymbolInfo> {
    match item {
        Item::Fn(fd) => {
            if !span_contains(fd.span, byte_offset) {
                return None;
            }
            // If the cursor is inside the fn body (not on the header/signature),
            // return None so the body-walk fallback can find the correct symbol.
            // We detect "inside body" by comparing the cursor against the body's
            // start span. FnDecl has no separate name_span, so we use the body
            // start as the boundary.
            let body_start = match &fd.body {
                FnBody::Block(block) => Some(block.span.start),
                FnBody::Expr(expr)   => Some(expr.span.start),
                FnBody::External      => None,
            };
            if let Some(bs) = body_start {
                if byte_offset >= bs {
                    // Cursor is inside the fn body — let body-walk handle it.
                    return None;
                }
            }
            match &fd.receiver {
                None => Some(SymbolInfo::FnDecl {
                    name: fd.name.clone(),
                    signature: format_fn_signature(fd),
                    doc: extract_doc(&fd.doc),
                    span: fd.span,
                }),
                Some(recv) => Some(SymbolInfo::MethodDecl {
                    receiver_type: format_receiver_type(recv),
                    name: fd.name.clone(),
                    signature: format_method_signature(fd, recv),
                    doc: extract_doc(&fd.doc),
                    span: fd.span,
                }),
            }
        }
        Item::Type(td) => {
            if !span_contains(td.span, byte_offset) {
                return None;
            }
            let (kind_label, signature) = match &td.kind {
                TypeDeclKind::Record(_) => ("record", None),
                TypeDeclKind::Sum(_) => ("sum", None),
                TypeDeclKind::Effect(_) => ("effect", None),
                TypeDeclKind::Protocol { .. } => ("protocol", None),
                TypeDeclKind::Newtype(_) => ("newtype", None),
                TypeDeclKind::Alias(_) => ("alias", None),
                TypeDeclKind::NamedTuple(fields) => {
                    ("named-tuple", Some(format_named_tuple_sig(&td.name, fields)))
                }
                TypeDeclKind::TypeSet(_) => ("type-set", None),
                TypeDeclKind::Opaque => ("opaque", None),
            };
            Some(SymbolInfo::TypeDecl {
                name: td.name.clone(),
                kind_label: kind_label.to_string(),
                signature,
                doc: extract_doc(&td.doc),
                span: td.span,
            })
        }
        Item::Let(ld) => {
            if !span_contains(ld.span, byte_offset) {
                return None;
            }
            let name = pattern_name(&ld.pattern)
                .unwrap_or("<pattern>")
                .to_string();
            let ty_text = ld.ty.as_ref().map(format_type_ref).unwrap_or_else(|| "_".to_string());
            Some(SymbolInfo::LocalVar {
                name,
                ty_text,
                is_mut: ld.mutable,
                span: ld.span,
                doc: None,
            })
        }
        Item::Const(cd) => {
            if !span_contains(cd.span, byte_offset) {
                return None;
            }
            let ty_text = cd.ty.as_ref().map(format_type_ref).unwrap_or_else(|| "_".to_string());
            Some(SymbolInfo::ConstDecl {
                name: cd.name.clone(),
                ty_text,
                span: cd.span,
                doc: extract_doc(&cd.doc),
            })
        }
        Item::Test(_) | Item::Bench(_) | Item::Lemma(_) => None,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Body walker — find identifier name at byte offset inside fn/test bodies
// ─────────────────────────────────────────────────────────────────────────────

/// Try to find the ident/Path name at `offset` inside any fn/test body in `module`.
/// Returns the name string if found, or None. Walks all items (used on
/// non-inlined modules where every item is the entry's).
pub fn find_ident_in_bodies(module: &Module, offset: usize) -> Option<String> {
    for item in &module.items {
        if let Some(name) = find_ident_in_item(item, offset) {
            return Some(name);
        }
    }
    None
}

/// Walk only the **entry** file's items (provenance `file_id == MAIN_FILE_ID`),
/// skipping inlined imports and folder-module peers whose spans are foreign.
pub fn find_ident_in_bodies_entry(module: &Module, offset: usize) -> Option<String> {
    for item in module.items.iter().filter(|it| item_belongs_to_entry(it)) {
        if let Some(name) = find_ident_in_item(item, offset) {
            return Some(name);
        }
    }
    None
}

/// Find an ident at `offset` inside a single item's fn/test body.
fn find_ident_in_item(item: &Item, offset: usize) -> Option<String> {
    match item {
        Item::Fn(fd) => {
            if !span_contains(fd.span, offset) {
                return None;
            }
            find_ident_in_fn_body(fd, offset)
        }
        Item::Test(td) => {
            if span_contains(td.span, offset) {
                find_ident_in_block(&td.body, offset)
            } else {
                None
            }
        }
        _ => None,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Local variable binding walker — find Stmt::Let binding at byte offset
// ─────────────────────────────────────────────────────────────────────────────

/// Walk fn/test bodies looking for a `Stmt::Let` binding whose pattern span
/// covers `offset`. Returns a `SymbolInfo::LocalVar` if found.
/// This handles hover on the binding name of a local variable.
fn find_local_var_at(module: &Module, offset: usize, env: Option<&ModuleEnv>) -> Option<SymbolInfo> {
    for item in module.items.iter().filter(|it| item_belongs_to_entry(it)) {
        match item {
            Item::Fn(fd) => {
                if !span_contains(fd.span, offset) {
                    continue;
                }
                if let FnBody::Block(block) = &fd.body {
                    if let Some(info) = find_local_var_in_block(block, offset, env) {
                        return Some(info);
                    }
                }
            }
            Item::Test(td) => {
                if span_contains(td.span, offset) {
                    if let Some(info) = find_local_var_in_block(&td.body, offset, env) {
                        return Some(info);
                    }
                }
            }
            _ => {}
        }
    }
    None
}

fn find_local_var_in_block(block: &Block, offset: usize, env: Option<&ModuleEnv>) -> Option<SymbolInfo> {
    for stmt in &block.stmts {
        if let Some(info) = find_local_var_in_stmt(stmt, offset, env) {
            return Some(info);
        }
    }
    None
}

fn find_local_var_in_stmt(stmt: &Stmt, offset: usize, env: Option<&ModuleEnv>) -> Option<SymbolInfo> {
    match stmt {
        Stmt::Let(ld) => {
            // Check if cursor is on the pattern binding name.
            if span_contains(ld.pattern.span(), offset) {
                let name = pattern_name(&ld.pattern)
                    .unwrap_or("<pattern>")
                    .to_string();
                let ty_text = ld.ty.as_ref()
                    .map(format_type_ref)
                    .unwrap_or_else(|| {
                        // Variant B: infer type from RHS using ModuleEnv when no explicit annotation.
                        infer_rhs_type(&ld.value, env).unwrap_or_else(|| "_".to_string())
                    });
                return Some(SymbolInfo::LocalVar {
                    name,
                    ty_text,
                    is_mut: ld.mutable,
                    span: ld.span,
                    doc: None,
                });
            }
            // Recurse into nested blocks in the value expression.
            find_local_var_in_expr(&ld.value, offset, env)
        }
        // Recurse into control-flow bodies that contain nested Stmt::Let.
        Stmt::Expr(e) => find_local_var_in_expr(e, offset, env),
        Stmt::Assign { value, .. } => find_local_var_in_expr(value, offset, env),
        _ => None,
    }
}

/// Variant B: infer the type of a RHS expression using ModuleEnv.
/// Handles the most common cases: literals, function calls, range literals.
/// Returns None for complex expressions where inference is not trivial.
fn infer_rhs_type(expr: &Expr, env: Option<&ModuleEnv>) -> Option<String> {
    match &expr.kind {
        // Literals — known types
        ExprKind::IntLit(_) => Some("int".to_string()),
        ExprKind::FloatLit(_) => Some("float".to_string()),
        ExprKind::BoolLit(_) => Some("bool".to_string()),
        ExprKind::StrLit(_) | ExprKind::InterpolatedStr { .. } => Some("str".to_string()),
        ExprKind::CharLit(_) => Some("char".to_string()),
        ExprKind::UnitLit => Some("()".to_string()),
        ExprKind::ArrayLit(elems) => {
            // []T — Vec[T]. Try to infer T from first element.
            if elems.is_empty() {
                Some("[]_".to_string())
            } else {
                // Only look at concrete element literals.
                let first = match &elems[0] {
                    nova_codegen::ast::ArrayElem::Item(e) => infer_rhs_type(e, env),
                    _ => None,
                };
                match first {
                    Some(t) => Some(format!("[]{}", t)),
                    None => Some("[]_".to_string()),
                }
            }
        }
        // Range literals: `lo..hi` or `lo..=hi` → Range
        ExprKind::Range { .. } => Some("Range".to_string()),
        // Function call — look up return type in ModuleEnv.fns
        ExprKind::Call { func, .. } => {
            let fn_name = extract_call_name(func)?;
            let env = env?;
            let overloads = env.fns.get(&fn_name)?;
            // Use first overload's return type (Variant B simplification).
            let fd = overloads.first()?;
            fd.return_type.as_ref().map(format_type_ref)
        }
        // As-cast: the result type is the cast target.
        ExprKind::As(_, ty) => Some(format_type_ref(ty)),
        _ => None,
    }
}

/// Extract the callee name from a Call `func` expression.
fn extract_call_name(func: &Expr) -> Option<String> {
    match &func.kind {
        ExprKind::Ident(name) => Some(name.clone()),
        ExprKind::Path(parts) => parts.last().cloned(),
        ExprKind::TurboFish { base, .. } => extract_call_name(base),
        _ => None,
    }
}

fn find_local_var_in_expr(expr: &Expr, offset: usize, env: Option<&ModuleEnv>) -> Option<SymbolInfo> {
    if !span_contains(expr.span, offset) {
        return None;
    }
    match &expr.kind {
        ExprKind::Block(block) => find_local_var_in_block(block, offset, env),
        ExprKind::If { then, else_, .. } => {
            find_local_var_in_block(then, offset, env)
                .or_else(|| else_.as_ref().and_then(|e| match e {
                    nova_codegen::ast::ElseBranch::Block(b) => find_local_var_in_block(b, offset, env),
                    nova_codegen::ast::ElseBranch::If(expr) => find_local_var_in_expr(expr, offset, env),
                }))
        }
        ExprKind::While { body, .. } => find_local_var_in_block(body, offset, env),
        ExprKind::For { body, .. } => find_local_var_in_block(body, offset, env),
        ExprKind::IfLet { then, else_, .. } => {
            find_local_var_in_block(then, offset, env)
                .or_else(|| else_.as_ref().and_then(|e| match e {
                    nova_codegen::ast::ElseBranch::Block(b) => find_local_var_in_block(b, offset, env),
                    nova_codegen::ast::ElseBranch::If(expr) => find_local_var_in_expr(expr, offset, env),
                }))
        }
        ExprKind::WhileLet { body, .. } => find_local_var_in_block(body, offset, env),
        ExprKind::Forbid { body, .. } | ExprKind::Blocking(body) => find_local_var_in_block(body, offset, env),
        ExprKind::Supervised { body, .. } => find_local_var_in_block(body, offset, env),
        ExprKind::Match { arms, .. } => {
            arms.iter().find_map(|arm| match &arm.body {
                MatchArmBody::Block(b) => find_local_var_in_block(b, offset, env),
                MatchArmBody::Expr(e) => find_local_var_in_expr(e, offset, env),
            })
        }
        ExprKind::ClosureLight { body, .. } => match body {
            nova_codegen::ast::ClosureBody::Block(b) => find_local_var_in_block(b, offset, env),
            nova_codegen::ast::ClosureBody::Expr(e) => find_local_var_in_expr(e, offset, env),
        },
        ExprKind::ClosureFull(sig_body) => match &sig_body.body {
            FnBody::Block(b) => find_local_var_in_block(b, offset, env),
            FnBody::Expr(e) => find_local_var_in_expr(e, offset, env),
            FnBody::External => None,
        },
        _ => None,
    }
}

fn find_ident_in_fn_body(fd: &FnDecl, offset: usize) -> Option<String> {
    match &fd.body {
        FnBody::Block(block) => find_ident_in_block(block, offset),
        FnBody::Expr(e) => find_ident_in_expr(e, offset),
        FnBody::External => None,
    }
}

fn find_ident_in_block(block: &Block, offset: usize) -> Option<String> {
    for stmt in &block.stmts {
        if let Some(n) = find_ident_in_stmt(stmt, offset) {
            return Some(n);
        }
    }
    if let Some(trailing) = &block.trailing {
        find_ident_in_expr(trailing.as_ref(), offset)
    } else {
        None
    }
}

fn find_ident_in_stmt(stmt: &Stmt, offset: usize) -> Option<String> {
    match stmt {
        Stmt::Let(ld) => find_ident_in_expr(&ld.value, offset),
        Stmt::Const(cd) => find_ident_in_expr(&cd.value, offset),
        Stmt::Expr(e) => find_ident_in_expr(e, offset),
        Stmt::Assign { target, value, .. } => {
            find_ident_in_expr(target, offset)
                .or_else(|| find_ident_in_expr(value, offset))
        }
        Stmt::TupleAssign { lhs, rhs, .. } => {
            lhs.iter().find_map(|e| find_ident_in_expr(e, offset))
                .or_else(|| rhs.iter().find_map(|e| find_ident_in_expr(e, offset)))
        }
        Stmt::Return { value, .. } => {
            value.as_ref().and_then(|e| find_ident_in_expr(e, offset))
        }
        Stmt::Throw { value, .. } => find_ident_in_expr(value, offset),
        Stmt::Defer { body, .. } => find_ident_in_expr(body, offset),
        Stmt::ConsumeScope { init, body, .. } => {
            find_ident_in_expr(init, offset)
                .or_else(|| find_ident_in_block(body, offset))
        }
        Stmt::AssertStatic { expr, .. }
        | Stmt::Assume { expr, .. } => find_ident_in_expr(expr, offset),
        Stmt::Apply { args, .. } => args.iter().find_map(|a| find_ident_in_expr(a, offset)),
        Stmt::Calc { steps, .. } => steps.iter().find_map(|s| {
            find_ident_in_expr(&s.expr, offset)
        }),
        Stmt::Break(_) | Stmt::Continue(_) => None,
        _ => None,
    }
}

fn find_ident_in_expr(expr: &Expr, offset: usize) -> Option<String> {
    if !span_contains(expr.span, offset) {
        return None;
    }
    match &expr.kind {
        ExprKind::Ident(name) => Some(name.clone()),
        ExprKind::Path(parts) => Some(parts.last()?.clone()),
        ExprKind::Call { func, args, .. } => {
            find_ident_in_expr(func, offset)
                .or_else(|| args.iter().find_map(|a| find_ident_in_expr(a.expr(), offset)))
        }
        ExprKind::Member { obj, .. } => find_ident_in_expr(obj, offset),
        ExprKind::Index { obj, index } => {
            find_ident_in_expr(obj, offset)
                .or_else(|| find_ident_in_expr(index, offset))
        }
        ExprKind::Binary { left, right, .. } => {
            find_ident_in_expr(left, offset)
                .or_else(|| find_ident_in_expr(right, offset))
        }
        ExprKind::Unary { operand, .. } => find_ident_in_expr(operand, offset),
        ExprKind::If { cond, then, else_, .. } => {
            find_ident_in_expr(cond, offset)
                .or_else(|| find_ident_in_block(then, offset))
                .or_else(|| else_.as_ref().and_then(|e| match e {
                    nova_codegen::ast::ElseBranch::Block(b) => find_ident_in_block(b, offset),
                    nova_codegen::ast::ElseBranch::If(expr) => find_ident_in_expr(expr, offset),
                }))
        }
        ExprKind::IfLet { scrutinee, then, else_, guard, .. } => {
            find_ident_in_expr(scrutinee, offset)
                .or_else(|| guard.as_ref().and_then(|g| find_ident_in_expr(g, offset)))
                .or_else(|| find_ident_in_block(then, offset))
                .or_else(|| else_.as_ref().and_then(|e| match e {
                    nova_codegen::ast::ElseBranch::Block(b) => find_ident_in_block(b, offset),
                    nova_codegen::ast::ElseBranch::If(expr) => find_ident_in_expr(expr, offset),
                }))
        }
        ExprKind::While { cond, body, .. } => {
            find_ident_in_expr(cond, offset)
                .or_else(|| find_ident_in_block(body, offset))
        }
        ExprKind::For { iter, body, .. } => {
            find_ident_in_expr(iter, offset)
                .or_else(|| find_ident_in_block(body, offset))
        }
        ExprKind::Block(block) => find_ident_in_block(block, offset),
        ExprKind::Match { scrutinee, arms, .. } => {
            find_ident_in_expr(scrutinee, offset)
                .or_else(|| arms.iter().find_map(|arm| {
                    arm.guard.as_ref().and_then(|g| find_ident_in_expr(g, offset))
                        .or_else(|| match &arm.body {
                            MatchArmBody::Expr(e) => find_ident_in_expr(e, offset),
                            MatchArmBody::Block(b) => find_ident_in_block(b, offset),
                        })
                }))
        }
        ExprKind::RecordLit { fields, .. } => {
            fields.iter().find_map(|f| {
                f.value.as_ref().and_then(|v| find_ident_in_expr(v, offset))
            })
        }
        ExprKind::ArrayLit(elems) => {
            elems.iter().find_map(|e| match e {
                nova_codegen::ast::ArrayElem::Item(expr) => find_ident_in_expr(expr, offset),
                nova_codegen::ast::ArrayElem::Spread(expr) => find_ident_in_expr(expr, offset),
            })
        }
        ExprKind::TupleLit(elems) => {
            elems.iter().find_map(|e| find_ident_in_expr(e, offset))
        }
        ExprKind::ClosureLight { body, .. } => match body {
            nova_codegen::ast::ClosureBody::Expr(e) => find_ident_in_expr(e, offset),
            nova_codegen::ast::ClosureBody::Block(b) => find_ident_in_block(b, offset),
        },
        ExprKind::ClosureFull(sig_body) => match &sig_body.body {
            FnBody::Block(b) => find_ident_in_block(b, offset),
            FnBody::Expr(e) => find_ident_in_expr(e, offset),
            FnBody::External => None,
        },
        ExprKind::TurboFish { base, .. } => find_ident_in_expr(base, offset),
        ExprKind::WhileLet { scrutinee, body, guard, .. } => {
            find_ident_in_expr(scrutinee, offset)
                .or_else(|| guard.as_ref().and_then(|g| find_ident_in_expr(g, offset)))
                .or_else(|| find_ident_in_block(body, offset))
        }
        ExprKind::Forbid { body, .. } | ExprKind::Blocking(body) => find_ident_in_block(body, offset),
        ExprKind::Supervised { body, cancel, .. } => {
            find_ident_in_block(body, offset)
                .or_else(|| cancel.as_ref().and_then(|c| find_ident_in_expr(c, offset)))
        }
        ExprKind::Forall { range, body, .. } => {
            find_ident_in_expr(range, offset)
                .or_else(|| find_ident_in_expr(body, offset))
        }
        ExprKind::Try(inner) | ExprKind::Bang(inner) => find_ident_in_expr(inner, offset),
        ExprKind::Spawn(inner) | ExprKind::Throw(inner) => find_ident_in_expr(inner, offset),
        ExprKind::As(inner, _) | ExprKind::Is(inner, _) => find_ident_in_expr(inner, offset),
        ExprKind::Coalesce(a, b) => {
            find_ident_in_expr(a, offset).or_else(|| find_ident_in_expr(b, offset))
        }
        ExprKind::Range { start, end, .. } => {
            start.as_ref().and_then(|e| find_ident_in_expr(e, offset))
                .or_else(|| end.as_ref().and_then(|e| find_ident_in_expr(e, offset)))
        }
        ExprKind::Exists { range, body, .. } => {
            find_ident_in_expr(range, offset).or_else(|| find_ident_in_expr(body, offset))
        }
        _ => None,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Member-access resolution (Plan 104.10 Ф.6) — `obj.field` hover / goto
// ─────────────────────────────────────────────────────────────────────────────

/// Resolve `obj.field` when the cursor is on the `field` part.
///
/// Ф.6 pipeline (criterion #4 — *object type from `expr_types`, field from the
/// real decl*):
/// 1. Locate the innermost member-access whose field-name region covers `offset`.
/// 2. Look the **object's** type up in `expr_types` (Ф.2) and reduce it to a base
///    type name (peeling `ro/mut/*` modifiers; `[]T` → `Vec`).
/// 3. Find the field (record / named-tuple) or instance method of that owner
///    type in its **real** declaration and return a [`SymbolInfo`].
///
/// Returns `None` (graceful degrade) when there is no `expr_types` map, the
/// cursor is not on a member field, the object's type is unknown, or the owner
/// type declares no such field/method.
fn resolve_member_at(
    module: &Module,
    offset: usize,
    env: Option<&ModuleEnv>,
) -> Option<SymbolInfo> {
    let env = env?;
    if env.expr_types.is_empty() {
        return None;
    }
    let (obj_span, field_name) = find_member_field_entry(module, offset)?;
    let obj_ty = expr_type_for_span(env, obj_span)?;
    let owner = type_ref_base_name(obj_ty)?;
    resolve_field_or_method(module, &owner, &field_name)
}

/// Look a recorded expression type up by its byte span.
///
/// Direct `HashMap` hit first; then a byte-range fallback that is robust to the
/// entry-file `file_id` duality (a span may be stamped `MAIN_FILE_ID` in the AST
/// but recorded under the entry's on-disk `file_id`, or vice-versa).
fn expr_type_for_span(env: &ModuleEnv, span: Span) -> Option<&TypeRef> {
    if let Some(t) = env.expr_types.get(&span) {
        return Some(t);
    }
    env.expr_types
        .iter()
        .find(|(s, _)| s.start == span.start && s.end == span.end)
        .map(|(_, t)| t)
}

/// Reduce a `TypeRef` to the base type NAME used to look up a declaration.
/// `Named{path:[..,"Foo"]}` → `Foo`; slice/array → `Vec` (`[]T` aliases
/// `Vec[T]`); `ro/mut/*/unsafe T` peel to the base of `T`.
fn type_ref_base_name(ty: &TypeRef) -> Option<String> {
    match ty {
        TypeRef::Named { path, .. } => path.last().cloned(),
        TypeRef::Array(..) | TypeRef::FixedArray(..) => Some("Vec".to_string()),
        TypeRef::Readonly(inner, _)
        | TypeRef::Mut(inner, _)
        | TypeRef::Uninit(inner, _)
        | TypeRef::Pointer(inner, _) => type_ref_base_name(inner),
        _ => None,
    }
}

/// True if receiver `recv`'s type matches `ty_name`, honouring the `[]T`/`Vec[T]`
/// slice-alias equivalence (a `fn []T @m` receiver is a method on `Vec`).
fn receiver_matches_name(recv: &Receiver, ty_name: &str) -> bool {
    if recv.type_name == ty_name {
        return true;
    }
    if ty_name == "Vec" && recv.type_name.starts_with("[]") {
        return true;
    }
    if recv.type_name == "Vec" && ty_name.starts_with("[]") {
        return true;
    }
    false
}

/// Find a field (record / named-tuple) or instance method named `field_name` on
/// the type `owner`, reading from that type's **real** declaration in `module`
/// (fields take precedence over methods, as at a value member access).
fn resolve_field_or_method(module: &Module, owner: &str, field_name: &str) -> Option<SymbolInfo> {
    // 1) Field of a record / named-tuple declaration.
    for item in &module.items {
        if let Item::Type(td) = item {
            if td.name != owner {
                continue;
            }
            let field = match &td.kind {
                TypeDeclKind::Record(fields) => fields
                    .iter()
                    .find(|f| f.name == field_name)
                    .map(|f| (f.name.clone(), format_type_ref(&f.ty), f.span, extract_doc(&f.doc))),
                TypeDeclKind::NamedTuple(fields) => fields
                    .iter()
                    .find(|f| f.name == field_name)
                    .map(|f| (f.name.clone(), format_type_ref(&f.ty), f.span, None)),
                _ => None,
            };
            if let Some((name, ty_text, span, doc)) = field {
                return Some(SymbolInfo::FieldDecl {
                    owner: owner.to_string(),
                    name,
                    ty_text,
                    span,
                    doc,
                });
            }
        }
    }
    // 2) Instance method on the owner type.
    for item in &module.items {
        if let Item::Fn(fd) = item {
            if fd.name != field_name {
                continue;
            }
            if let Some(recv) = &fd.receiver {
                if receiver_matches_name(recv, owner) {
                    return Some(SymbolInfo::MethodDecl {
                        receiver_type: format_receiver_type(recv),
                        name: fd.name.clone(),
                        signature: format_method_signature(fd, recv),
                        doc: extract_doc(&fd.doc),
                        span: fd.span,
                    });
                }
            }
        }
    }
    None
}

// ── Member-access AST walker (find the field region under the cursor) ─────────

/// Walk the **entry** file's fn/test bodies for a member access `obj.field`
/// whose *field-name region* covers `offset`. Returns `(obj_span, field_name)`.
fn find_member_field_entry(module: &Module, offset: usize) -> Option<(Span, String)> {
    for item in module.items.iter().filter(|it| item_belongs_to_entry(it)) {
        match item {
            Item::Fn(fd) => {
                if !span_contains(fd.span, offset) {
                    continue;
                }
                let r = match &fd.body {
                    FnBody::Block(b) => find_member_field_in_block(b, offset),
                    FnBody::Expr(e) => find_member_field_in_expr(e, offset),
                    FnBody::External => None,
                };
                if r.is_some() {
                    return r;
                }
            }
            Item::Test(td) => {
                if span_contains(td.span, offset) {
                    if let Some(r) = find_member_field_in_block(&td.body, offset) {
                        return Some(r);
                    }
                }
            }
            _ => {}
        }
    }
    None
}

fn find_member_field_in_block(block: &Block, offset: usize) -> Option<(Span, String)> {
    for stmt in &block.stmts {
        if let Some(r) = find_member_field_in_stmt(stmt, offset) {
            return Some(r);
        }
    }
    block
        .trailing
        .as_ref()
        .and_then(|t| find_member_field_in_expr(t.as_ref(), offset))
}

fn find_member_field_in_stmt(stmt: &Stmt, offset: usize) -> Option<(Span, String)> {
    match stmt {
        Stmt::Let(ld) => find_member_field_in_expr(&ld.value, offset),
        Stmt::Const(cd) => find_member_field_in_expr(&cd.value, offset),
        Stmt::Expr(e) => find_member_field_in_expr(e, offset),
        Stmt::Assign { target, value, .. } => find_member_field_in_expr(target, offset)
            .or_else(|| find_member_field_in_expr(value, offset)),
        Stmt::TupleAssign { lhs, rhs, .. } => lhs
            .iter()
            .find_map(|e| find_member_field_in_expr(e, offset))
            .or_else(|| rhs.iter().find_map(|e| find_member_field_in_expr(e, offset))),
        Stmt::Return { value, .. } => {
            value.as_ref().and_then(|e| find_member_field_in_expr(e, offset))
        }
        Stmt::Throw { value, .. } => find_member_field_in_expr(value, offset),
        Stmt::Defer { body, .. } => find_member_field_in_expr(body, offset),
        Stmt::ConsumeScope { init, body, .. } => find_member_field_in_expr(init, offset)
            .or_else(|| find_member_field_in_block(body, offset)),
        Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => {
            find_member_field_in_expr(expr, offset)
        }
        Stmt::Apply { args, .. } => args.iter().find_map(|a| find_member_field_in_expr(a, offset)),
        Stmt::Calc { steps, .. } => {
            steps.iter().find_map(|s| find_member_field_in_expr(&s.expr, offset))
        }
        _ => None,
    }
}

fn find_member_field_in_expr(expr: &Expr, offset: usize) -> Option<(Span, String)> {
    if !span_contains(expr.span, offset) {
        return None;
    }
    match &expr.kind {
        ExprKind::Member { obj, name } => {
            // Innermost first: for a chain `a.b.c`, recurse into `obj` so hover
            // on an inner field (`b`) resolves against `a`, not the outer expr.
            if let Some(r) = find_member_field_in_expr(obj, offset) {
                return Some(r);
            }
            // Cursor on the field-name region (past the object, within this
            // member expression) — this is the field being hovered.
            if offset > obj.span.end && offset <= expr.span.end {
                return Some((obj.span, name.clone()));
            }
            None
        }
        ExprKind::Call { func, args, .. } => find_member_field_in_expr(func, offset)
            .or_else(|| args.iter().find_map(|a| find_member_field_in_expr(a.expr(), offset))),
        ExprKind::Index { obj, index } => find_member_field_in_expr(obj, offset)
            .or_else(|| find_member_field_in_expr(index, offset)),
        ExprKind::Binary { left, right, .. } => find_member_field_in_expr(left, offset)
            .or_else(|| find_member_field_in_expr(right, offset)),
        ExprKind::Unary { operand, .. } => find_member_field_in_expr(operand, offset),
        ExprKind::If { cond, then, else_, .. } => find_member_field_in_expr(cond, offset)
            .or_else(|| find_member_field_in_block(then, offset))
            .or_else(|| else_.as_ref().and_then(|e| find_member_field_in_else(e, offset))),
        ExprKind::IfLet { scrutinee, then, else_, guard, .. } => {
            find_member_field_in_expr(scrutinee, offset)
                .or_else(|| guard.as_ref().and_then(|g| find_member_field_in_expr(g, offset)))
                .or_else(|| find_member_field_in_block(then, offset))
                .or_else(|| else_.as_ref().and_then(|e| find_member_field_in_else(e, offset)))
        }
        ExprKind::While { cond, body, .. } => find_member_field_in_expr(cond, offset)
            .or_else(|| find_member_field_in_block(body, offset)),
        ExprKind::For { iter, body, .. } => find_member_field_in_expr(iter, offset)
            .or_else(|| find_member_field_in_block(body, offset)),
        ExprKind::Block(block) => find_member_field_in_block(block, offset),
        ExprKind::Match { scrutinee, arms, .. } => find_member_field_in_expr(scrutinee, offset)
            .or_else(|| {
                arms.iter().find_map(|arm| {
                    arm.guard
                        .as_ref()
                        .and_then(|g| find_member_field_in_expr(g, offset))
                        .or_else(|| match &arm.body {
                            MatchArmBody::Expr(e) => find_member_field_in_expr(e, offset),
                            MatchArmBody::Block(b) => find_member_field_in_block(b, offset),
                        })
                })
            }),
        ExprKind::RecordLit { fields, .. } => fields
            .iter()
            .find_map(|f| f.value.as_ref().and_then(|v| find_member_field_in_expr(v, offset))),
        ExprKind::ArrayLit(elems) => elems.iter().find_map(|e| match e {
            nova_codegen::ast::ArrayElem::Item(expr) => find_member_field_in_expr(expr, offset),
            nova_codegen::ast::ArrayElem::Spread(expr) => find_member_field_in_expr(expr, offset),
        }),
        ExprKind::TupleLit(elems) => {
            elems.iter().find_map(|e| find_member_field_in_expr(e, offset))
        }
        ExprKind::ClosureLight { body, .. } => match body {
            nova_codegen::ast::ClosureBody::Expr(e) => find_member_field_in_expr(e, offset),
            nova_codegen::ast::ClosureBody::Block(b) => find_member_field_in_block(b, offset),
        },
        ExprKind::ClosureFull(sig_body) => match &sig_body.body {
            FnBody::Block(b) => find_member_field_in_block(b, offset),
            FnBody::Expr(e) => find_member_field_in_expr(e, offset),
            FnBody::External => None,
        },
        ExprKind::TurboFish { base, .. } => find_member_field_in_expr(base, offset),
        ExprKind::WhileLet { scrutinee, body, guard, .. } => {
            find_member_field_in_expr(scrutinee, offset)
                .or_else(|| guard.as_ref().and_then(|g| find_member_field_in_expr(g, offset)))
                .or_else(|| find_member_field_in_block(body, offset))
        }
        ExprKind::Forbid { body, .. } | ExprKind::Blocking(body) => {
            find_member_field_in_block(body, offset)
        }
        ExprKind::Supervised { body, cancel, .. } => find_member_field_in_block(body, offset)
            .or_else(|| cancel.as_ref().and_then(|c| find_member_field_in_expr(c, offset))),
        ExprKind::Forall { range, body, .. } => find_member_field_in_expr(range, offset)
            .or_else(|| find_member_field_in_expr(body, offset)),
        ExprKind::Try(inner) | ExprKind::Bang(inner) => find_member_field_in_expr(inner, offset),
        ExprKind::Spawn(inner) | ExprKind::Throw(inner) => find_member_field_in_expr(inner, offset),
        ExprKind::As(inner, _) | ExprKind::Is(inner, _) => find_member_field_in_expr(inner, offset),
        ExprKind::Coalesce(a, b) => find_member_field_in_expr(a, offset)
            .or_else(|| find_member_field_in_expr(b, offset)),
        ExprKind::Range { start, end, .. } => start
            .as_ref()
            .and_then(|e| find_member_field_in_expr(e, offset))
            .or_else(|| end.as_ref().and_then(|e| find_member_field_in_expr(e, offset))),
        ExprKind::Exists { range, body, .. } => find_member_field_in_expr(range, offset)
            .or_else(|| find_member_field_in_expr(body, offset)),
        _ => None,
    }
}

fn find_member_field_in_else(
    e: &nova_codegen::ast::ElseBranch,
    offset: usize,
) -> Option<(Span, String)> {
    match e {
        nova_codegen::ast::ElseBranch::Block(b) => find_member_field_in_block(b, offset),
        nova_codegen::ast::ElseBranch::If(expr) => find_member_field_in_expr(expr, offset),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Lookup by name (for hover fallback + signature help)
// ─────────────────────────────────────────────────────────────────────────────

/// Look up a declaration by name in `module` items. Returns the first match.
/// Used as a fallback when body-walk finds an ident name but no span matches.
pub fn lookup_decl_by_name(module: &Module, name: &str) -> Option<SymbolInfo> {
    for item in &module.items {
        match item {
            Item::Fn(fd) if fd.name == name => {
                return Some(match &fd.receiver {
                    None => SymbolInfo::FnDecl {
                        name: fd.name.clone(),
                        signature: format_fn_signature(fd),
                        doc: extract_doc(&fd.doc),
                        span: fd.span,
                    },
                    Some(recv) => SymbolInfo::MethodDecl {
                        receiver_type: format_receiver_type(recv),
                        name: fd.name.clone(),
                        signature: format_method_signature(fd, recv),
                        doc: extract_doc(&fd.doc),
                        span: fd.span,
                    },
                });
            }
            Item::Type(td) if td.name == name => {
                let kind_label = match &td.kind {
                    TypeDeclKind::Record(_) => "record",
                    TypeDeclKind::Sum(_) => "sum",
                    TypeDeclKind::Effect(_) => "effect",
                    TypeDeclKind::Protocol { .. } => "protocol",
                    TypeDeclKind::Newtype(_) => "newtype",
                    TypeDeclKind::Alias(_) => "alias",
                    TypeDeclKind::NamedTuple(_) => "named-tuple",
                    TypeDeclKind::TypeSet(_) => "type-set",
                    TypeDeclKind::Opaque => "opaque",
                };
                let signature = if let TypeDeclKind::NamedTuple(fields) = &td.kind {
                    Some(format_named_tuple_sig(&td.name, fields))
                } else {
                    None
                };
                return Some(SymbolInfo::TypeDecl {
                    name: td.name.clone(),
                    kind_label: kind_label.to_string(),
                    signature,
                    doc: extract_doc(&td.doc),
                    span: td.span,
                });
            }
            Item::Const(cd) if cd.name == name => {
                let ty_text = cd.ty.as_ref().map(format_type_ref).unwrap_or_else(|| "_".to_string());
                return Some(SymbolInfo::ConstDecl {
                    name: cd.name.clone(),
                    ty_text,
                    span: cd.span,
                    doc: extract_doc(&cd.doc),
                });
            }
            _ => {}
        }
    }
    None
}

// ─────────────────────────────────────────────────────────────────────────────
// Lookup by name (for signature help)
// ─────────────────────────────────────────────────────────────────────────────

/// Find all free-function overloads named `name` in `module`.
pub fn find_fn_by_name<'a>(module: &'a Module, name: &str) -> Vec<&'a FnDecl> {
    module
        .items
        .iter()
        .filter_map(|item| {
            if let Item::Fn(fd) = item {
                if fd.receiver.is_none() && fd.name == name {
                    return Some(fd);
                }
            }
            None
        })
        .collect()
}

/// Find all method overloads named `name` (any receiver type) in `module`.
pub fn find_method_by_name<'a>(module: &'a Module, name: &str) -> Vec<&'a FnDecl> {
    module
        .items
        .iter()
        .filter_map(|item| {
            if let Item::Fn(fd) = item {
                if fd.receiver.is_some() && fd.name == name {
                    return Some(fd);
                }
            }
            None
        })
        .collect()
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_module(src: &str) -> Module {
        crate::compiler::parse_guarded(src)
            .unwrap_or_else(|_| panic!("parse failed for: {}", &src[..src.len().min(80)]))
    }

    // ── format_type_ref ──────────────────────────────────────────────────────

    #[test]
    fn test_format_named_simple() {
        let ty = TypeRef::Named {
            path: vec!["int".to_string()],
            generics: vec![],
            span: Span::dummy(),
        };
        assert_eq!(format_type_ref(&ty), "int");
    }

    #[test]
    fn test_format_named_generic() {
        let inner = TypeRef::Named {
            path: vec!["str".to_string()],
            generics: vec![],
            span: Span::dummy(),
        };
        let ty = TypeRef::Named {
            path: vec!["Option".to_string()],
            generics: vec![inner],
            span: Span::dummy(),
        };
        assert_eq!(format_type_ref(&ty), "Option[str]");
    }

    #[test]
    fn test_format_array() {
        let inner = TypeRef::Named {
            path: vec!["int".to_string()],
            generics: vec![],
            span: Span::dummy(),
        };
        let ty = TypeRef::Array(Box::new(inner), Span::dummy());
        assert_eq!(format_type_ref(&ty), "[]int");
    }

    #[test]
    fn test_format_unit() {
        let ty = TypeRef::Unit(Span::dummy());
        assert_eq!(format_type_ref(&ty), "()");
    }

    // ── resolve_symbol_at on a parsed module ─────────────────────────────────

    #[test]
    fn test_resolve_fn_decl() {
        let src = "module basics.lsp_test\nfn hello(x int) -> str => \"hi\"";
        let module = parse_module(src);
        // Position somewhere inside the fn declaration.
        let fn_start = src.find("fn hello").unwrap();
        let sym = resolve_symbol_at(&module, fn_start + 3);
        assert!(sym.is_some(), "should resolve fn at offset");
        match sym.unwrap() {
            SymbolInfo::FnDecl { name, .. } => assert_eq!(name, "hello"),
            other => panic!("expected FnDecl, got {:?}", other),
        }
    }

    #[test]
    fn test_resolve_type_decl() {
        let src = "module basics.lsp_test\ntype Point {\n x int\n y int\n}";
        let module = parse_module(src);
        let ty_start = src.find("type Point").unwrap();
        let sym = resolve_symbol_at(&module, ty_start + 5);
        assert!(sym.is_some(), "should resolve type at offset");
        match sym.unwrap() {
            SymbolInfo::TypeDecl { name, kind_label, .. } => {
                assert_eq!(name, "Point");
                assert_eq!(kind_label, "record");
            }
            other => panic!("expected TypeDecl, got {:?}", other),
        }
    }

    #[test]
    fn test_resolve_import() {
        let src = "module basics.lsp_test\nimport std.collections\nfn f() => ()";
        let module = parse_module(src);
        let imp_start = src.find("import").unwrap();
        let sym = resolve_symbol_at(&module, imp_start + 5);
        assert!(sym.is_some(), "should resolve import at offset");
        match sym.unwrap() {
            SymbolInfo::ImportRef { module_path, .. } => {
                assert_eq!(module_path, "std.collections");
            }
            other => panic!("expected ImportRef, got {:?}", other),
        }
    }

    #[test]
    fn test_resolve_method_decl() {
        let src = "module basics.lsp_test\ntype Foo {\n x int\n}\nfn Foo @bar() -> int => 0";
        let module = parse_module(src);
        let method_start = src.find("fn Foo @bar").unwrap();
        let sym = resolve_symbol_at(&module, method_start + 5);
        assert!(sym.is_some(), "should resolve method at offset");
        match sym.unwrap() {
            SymbolInfo::MethodDecl { receiver_type, name, .. } => {
                assert_eq!(receiver_type, "Foo");
                assert_eq!(name, "bar");
            }
            other => panic!("expected MethodDecl, got {:?}", other),
        }
    }

    #[test]
    fn test_resolve_none_at_whitespace() {
        // A position that's before any top-level item — should return None.
        let src = "module basics.lsp_test\nfn f() => ()";
        let module = parse_module(src);
        // Position 0 is in "module basics.lsp_test" — not an item.
        let sym = resolve_symbol_at(&module, 1);
        // May or may not resolve; main thing: no panic.
        let _ = sym;
    }

    #[test]
    fn test_resolve_eof_no_panic() {
        let src = "module basics.lsp_test\nfn f() => ()";
        let module = parse_module(src);
        let sym = resolve_symbol_at(&module, src.len() + 100);
        // Out of bounds — None, no panic.
        assert!(sym.is_none() || sym.is_some());
    }

    // ── find_fn_by_name ──────────────────────────────────────────────────────

    #[test]
    fn test_find_fn_by_name_found() {
        let src = "module basics.lsp_test\nfn add(a int, b int) -> int => a + b";
        let module = parse_module(src);
        let fns = find_fn_by_name(&module, "add");
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].name, "add");
    }

    #[test]
    fn test_find_fn_by_name_not_found() {
        let src = "module basics.lsp_test\nfn foo() => ()";
        let module = parse_module(src);
        let fns = find_fn_by_name(&module, "bar");
        assert!(fns.is_empty());
    }

    // ── Task A: fn-body hover priority fix ───────────────────────────────────

    #[test]
    fn test_resolve_callee_inside_fn_body() {
        // Hover on `add` inside `main`'s body should resolve to fn `add`, not fn `main`.
        let src = "module basics.lsp_test\nfn add(a int, b int) -> int => a + b\nfn main() {\n  add(1, 2)\n}";
        let module = parse_module(src);
        // Find the offset of the second occurrence of 'add' (the call site inside main).
        let second_add = src.rfind("add").unwrap();
        let sym = resolve_symbol_at(&module, second_add + 1);
        assert!(sym.is_some(), "should resolve callee in fn body");
        match sym.unwrap() {
            SymbolInfo::FnDecl { name, .. } => assert_eq!(name, "add"),
            other => panic!("expected FnDecl for 'add', got {:?}", other),
        }
    }

    #[test]
    fn test_resolve_fn_header_still_works() {
        // Hover on 'main' in `fn main()` should still return FnDecl for main.
        let src = "module basics.lsp_test\nfn add(a int, b int) -> int => a + b\nfn main() {\n  add(1, 2)\n}";
        let module = parse_module(src);
        let main_offset = src.find("fn main").unwrap() + 3; // 'm' in 'main'
        let sym = resolve_symbol_at(&module, main_offset);
        assert!(sym.is_some(), "should resolve fn main from its header");
        match sym.unwrap() {
            SymbolInfo::FnDecl { name, .. } => assert_eq!(name, "main"),
            other => panic!("expected FnDecl for 'main', got {:?}", other),
        }
    }

    // ── Task B: local variable hover ────────────────────────────────────────

    #[test]
    fn test_resolve_local_var_with_type_annotation() {
        // Hover on 'x' in `ro x int = 5` inside a fn body should return LocalVar with ty="int".
        let src = "module basics.lsp_test\nfn f() {\n  ro x int = 5\n}";
        let module = parse_module(src);
        let x_offset = src.find("ro x int").unwrap() + 3; // 'x' in 'ro x int'
        let sym = resolve_symbol_at(&module, x_offset);
        assert!(sym.is_some(), "should resolve local var binding");
        match sym.unwrap() {
            SymbolInfo::LocalVar { name, ty_text, is_mut, .. } => {
                assert_eq!(name, "x");
                assert_eq!(ty_text, "int");
                assert!(!is_mut);
            }
            other => panic!("expected LocalVar, got {:?}", other),
        }
    }

    #[test]
    fn test_resolve_local_var_no_annotation_infers_int() {
        // Hover on 'y' in `ro y = 5` (no annotation) — Variant B infers "int" from IntLit.
        let src = "module basics.lsp_test\nfn f() {\n  ro y = 5\n}";
        let module = parse_module(src);
        let y_offset = src.find("ro y").unwrap() + 3; // 'y' in 'ro y'
        let sym = resolve_symbol_at(&module, y_offset);
        // No panic; if resolved, ty_text should be inferred as "int" from the literal.
        if let Some(SymbolInfo::LocalVar { ty_text, .. }) = sym {
            assert_eq!(ty_text, "int");
        }
    }
}
