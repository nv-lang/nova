//! Symbol resolution for hover, goto-definition, and signature help.
//!
//! Plan 104.2.Ф.1: SymbolInfo enum + resolve_symbol_at + TypeRef formatter.
//!
//! The resolver walks the parsed AST to find the most specific symbol
//! covering a given byte offset. No cross-file resolution in V1
//! (deferred to Plan 104.4 / [M-104.2-cross-file-goto]).

use nova_codegen::ast::{
    Block, Expr, ExprKind, FnBody, FnDecl, Item, MatchArmBody,
    Module, Param, Pattern, Receiver, ReceiverKind,
    Stmt, TypeDeclKind, TypeRef,
};
use nova_codegen::diag::Span;

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
        TypeRef::Unsafe(inner, _) => {
            format!("unsafe {}", format_type_ref(inner))
        }
        TypeRef::Protocol { methods, .. } => {
            format!("protocol {{ {} method(s) }}", methods.len())
        }
    }
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
    resolve_symbol_at_with_limit(module, byte_offset, 0)
}

/// Resolve a symbol at `byte_offset` in `module`.
///
/// After `resolve_imports_inline`, imported items are **prepended** to
/// `module.items` (see imports.rs line ~829: `new_items.append(&mut module.items)`).
/// So the original file's items start at index `items_start` (= number of
/// imported items inserted), not at 0.
///
/// - Span-match is restricted to `module.items[items_start..]` (original items
///   only — inlined items have spans from other files).
/// - Body-walk is also restricted to `items[items_start..]`.
/// - Name lookup (to resolve the found ident) searches ALL items so prelude
///   symbols like `assert` are found.
pub fn resolve_symbol_at_with_limit(module: &Module, byte_offset: usize, items_start: usize) -> Option<SymbolInfo> {
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

    // Span-match only original items (not inlined ones that have foreign spans).
    for item in module.items.iter().skip(items_start) {
        if let Some(info) = resolve_item(item, byte_offset) {
            return Some(info);
        }
    }

    // Fallback: body-walk original items to find ident name at cursor,
    // then look it up by name across ALL items (including inlined prelude).
    if let Some(ident_name) = find_ident_in_bodies_from(module, byte_offset, items_start) {
        if let Some(info) = lookup_decl_by_name(module, &ident_name) {
            return Some(info);
        }
    }

    None
}

fn resolve_item(item: &Item, byte_offset: usize) -> Option<SymbolInfo> {
    match item {
        Item::Fn(fd) => {
            if !span_contains(fd.span, byte_offset) {
                return None;
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
            let kind_label = match &td.kind {
                TypeDeclKind::Record(_) => "record",
                TypeDeclKind::Sum(_) => "sum",
                TypeDeclKind::Effect(_) => "effect",
                TypeDeclKind::Protocol { .. } => "protocol",
                TypeDeclKind::Newtype(_) => "newtype",
                TypeDeclKind::Alias(_) => "alias",
                TypeDeclKind::NamedTuple(_) => "named-tuple",
                TypeDeclKind::Opaque => "opaque",
            };
            Some(SymbolInfo::TypeDecl {
                name: td.name.clone(),
                kind_label: kind_label.to_string(),
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
/// Returns the name string if found, or None.
pub fn find_ident_in_bodies(module: &Module, offset: usize) -> Option<String> {
    find_ident_in_bodies_from(module, offset, 0)
}

/// Walk only `module.items[items_start..]` (original file items, skipping inlined imports).
pub fn find_ident_in_bodies_from(module: &Module, offset: usize, items_start: usize) -> Option<String> {
    for item in module.items.iter().skip(items_start) {
        match item {
            Item::Fn(fd) => {
                if !span_contains(fd.span, offset) {
                    continue;
                }
                if let Some(name) = find_ident_in_fn_body(fd, offset) {
                    return Some(name);
                }
            }
            Item::Test(td) => {
                if span_contains(td.span, offset) {
                    if let Some(name) = find_ident_in_block(&td.body, offset) {
                        return Some(name);
                    }
                }
            }
            _ => {}
        }
    }
    None
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
        Stmt::Defer { body, .. }
        | Stmt::ErrDefer { body, .. }
        | Stmt::OkDefer { body, .. } => find_ident_in_expr(body, offset),
        Stmt::DeferWithResult { body, .. } => find_ident_in_expr(body, offset),
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
                    TypeDeclKind::Opaque => "opaque",
                };
                return Some(SymbolInfo::TypeDecl {
                    name: td.name.clone(),
                    kind_label: kind_label.to_string(),
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
        nova_codegen::parser::parse(src)
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
}
