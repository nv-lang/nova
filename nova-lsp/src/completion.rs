//! LSP completion provider — Plan 104.3.
//!
//! # Sub-plans implemented
//! - 104.3.1: Keyword + snippet completion (context-aware)
//! - 104.3.2: In-scope identifier completion (scope walk)
//! - 104.3.3: Method-dot completion (type-driven via ModuleEnv)
//! - 104.3.4: Import path completion (std module tree)
//! - 104.3.5: Ranking polish (locals > module > std > prelude)
//!
//! # Context detection
//!
//! Before generating completions, the cursor position is classified into one of:
//! `TopLevel | FnBody | TypeBody | Import | MethodDot { obj_text }`.
//! If the cursor is inside a comment or string literal, `None` is returned.
//!
//! # Performance
//!
//! All work is synchronous and runs inside `run_with_large_stack` in the server
//! handler. Must complete in ≤200ms for a typical file.
//!
//! # Lazy completion resolve (Plan 104.10 Ф.13)
//!
//! The initial completion list is kept lightweight: verbose `detail` /
//! `documentation` for the static families (keyword, snippet, prelude) and the
//! `documentation` markdown for methods/imports are OMITTED from the initial
//! response. Each such item carries a compact `data` descriptor (family tag +
//! lookup key). The client re-requests the heavy fields per item via
//! `completionItem/resolve`, dispatched to [`resolve_completion_item`], which
//! re-derives them from the static tables (keyword/snippet/prelude/import) with
//! zero extra allocation in the initial list, or reads a stashed doc string
//! (methods). `resolve_provider=true` is advertised in the server capabilities.
//!
//! # V1 simplifications (documented in simplifications.md)
//! - [S-104.3-1] Method dot: type inference via text pattern match, not full TypeCheckCtx.
//! - [S-104.3-2] Import path: hardcoded std module tree.
//! - [M-104.10-lsp-resolve-method-doc] Method-completion documentation is stashed
//!   in the item's `data` (already computed while resolving the module for the
//!   list) rather than re-resolved on demand — the wire payload is unchanged for
//!   the (few) method items, but rendering is still deferred to resolve. The
//!   static families (keyword/snippet/prelude/import), which dominate the list,
//!   are genuinely re-derived from tables so their heavy text never ships in the
//!   initial response.

use std::path::{Path, PathBuf};

use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, Documentation, InsertTextFormat, MarkupContent,
    MarkupKind,
};

use nova_codegen::ast::{Item, Module, TypeRef};
use nova_codegen::diag::MAIN_FILE_ID;
use nova_codegen::types::ModuleEnv;

use crate::provenance::{self, ResolvedModule};
use crate::stdlib_index::StdlibIndex;

// ─────────────────────────────────────────────────────────────────────────────
// Sort-text rank prefixes (lower string sorts first in editor dropdown)
// ─────────────────────────────────────────────────────────────────────────────

const RANK_LOCAL: &str = "00_";
const RANK_MODULE: &str = "01_";
const RANK_STD: &str = "02_";
const RANK_PRELUDE: &str = "03_";
const RANK_KEYWORD: &str = "04_";
const RANK_SNIPPET: &str = "05_";

// ─────────────────────────────────────────────────────────────────────────────
// Context
// ─────────────────────────────────────────────────────────────────────────────

/// Completion context derived from cursor position.
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum CompletionContext {
    /// Cursor at module top level (not inside fn/type body).
    TopLevel,
    /// Cursor inside a function body (between `{` and matching `}`).
    FnBody,
    /// Cursor inside a type body.
    TypeBody,
    /// Cursor on an import path: `import std.collections.│`.
    Import {
        /// Portion of the path before the cursor (e.g. `["std", "collections"]`).
        path_prefix: Vec<String>,
    },
    /// Cursor after a `.` for method call: `expr.│`.
    MethodDot {
        /// Text of the expression before the dot (e.g. `"x"`, `"my_vec"`).
        obj_text: String,
    },
}

/// Detect the completion context for `offset` in `src`.
///
/// Returns `None` if the cursor is inside a comment or string literal —
/// in that case no completion should be provided.
pub fn detect_context(src: &str, offset: usize) -> Option<CompletionContext> {
    // Clamp offset to valid range.
    let offset = offset.min(src.len());
    if is_in_comment(src, offset) || is_in_string(src, offset) {
        return None;
    }

    // Check for import path on the current line.
    let line_start = find_line_start(src, offset);
    let line_text = &src[line_start..offset];
    let trimmed = line_text.trim_start();
    if trimmed.starts_with("import ") || trimmed.starts_with("export import ") {
        // Extract path up to cursor.
        let path_part = if trimmed.starts_with("export import ") {
            &trimmed["export import ".len()..]
        } else {
            &trimmed["import ".len()..]
        };
        // Strip optional import items list (curly braces) — focus on the path.
        let path_str = path_part.split('{').next().unwrap_or("").trim();
        let path_prefix: Vec<String> = path_str
            .split('.')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();
        return Some(CompletionContext::Import { path_prefix });
    }

    // Check for method-dot: line ends with `identifier.` or `)`.` etc.
    if let Some(obj_text) = detect_method_dot(src, offset) {
        return Some(CompletionContext::MethodDot { obj_text });
    }

    // Determine if we're inside a fn body or type body by scanning backwards.
    match classify_brace_context(src, offset) {
        BraceContext::FnBody => Some(CompletionContext::FnBody),
        BraceContext::TypeBody => Some(CompletionContext::TypeBody),
        BraceContext::TopLevel => Some(CompletionContext::TopLevel),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Brace context classification
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, PartialEq, Eq)]
enum BraceContext {
    FnBody,
    TypeBody,
    TopLevel,
}

/// Walk backwards from `offset` tracking brace depth.
/// Determine whether cursor is in a fn body, type body, or top level.
fn classify_brace_context(src: &str, offset: usize) -> BraceContext {
    let bytes = src.as_bytes();
    let end = offset.min(bytes.len());
    let mut depth: i32 = 0;
    let mut i = end;

    while i > 0 {
        i -= 1;
        match bytes[i] {
            b'}' => depth += 1,
            b'{' => {
                if depth == 0 {
                    // We found the opening brace that encloses the cursor.
                    // Walk back from i to find what keyword preceded this brace.
                    let before = &src[..i];
                    let keyword = last_significant_keyword(before);
                    return match keyword {
                        Some("fn") | Some("test") | Some("bench") | Some("lemma") => {
                            BraceContext::FnBody
                        }
                        Some("type") | Some("effect") | Some("protocol") => {
                            BraceContext::TypeBody
                        }
                        _ => BraceContext::FnBody, // unknown enclosure → default fn body
                    };
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    BraceContext::TopLevel
}

/// Find the last Nova keyword before position `end` in `text`.
fn last_significant_keyword(text: &str) -> Option<&'static str> {
    // Walk backwards word-by-word.
    let bytes = text.as_bytes();
    let mut i = bytes.len();
    // Skip whitespace from the end.
    while i > 0 && bytes[i - 1].is_ascii_whitespace() {
        i -= 1;
    }
    // Find the last word.
    let word_end = i;
    while i > 0 && (bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'_') {
        i -= 1;
    }
    if i == word_end {
        // Nothing — try a few lines back.
        // Simple: search for the last `fn` or `type` or `test` etc. keyword.
        return find_last_decl_keyword(text);
    }
    let word = &text[i..word_end];
    match word {
        "fn" | "type" | "effect" | "protocol" | "test" | "bench" | "lemma" => {
            Some(match word {
                "fn" => "fn",
                "type" => "type",
                "effect" => "effect",
                "protocol" => "protocol",
                "test" => "test",
                "bench" => "bench",
                "lemma" => "lemma",
                _ => unreachable!(),
            })
        }
        _ => find_last_decl_keyword(text),
    }
}

/// Find the last decl keyword (`fn`, `type`, `test`, ...) anywhere in `text`.
fn find_last_decl_keyword(text: &str) -> Option<&'static str> {
    const DECL_KWS: &[&str] = &[
        "fn", "type", "effect", "protocol", "test", "bench", "lemma",
    ];
    let mut best_pos = 0usize;
    let mut best_kw: Option<&'static str> = None;
    for kw in DECL_KWS {
        // Find last occurrence.
        let mut start = 0;
        while let Some(pos) = text[start..].find(kw) {
            let abs_pos = start + pos;
            // Verify word boundary.
            let before_ok = abs_pos == 0
                || !text.as_bytes()[abs_pos - 1].is_ascii_alphanumeric();
            let after_ok = abs_pos + kw.len() >= text.len()
                || !text.as_bytes()[abs_pos + kw.len()].is_ascii_alphanumeric();
            if before_ok && after_ok && abs_pos >= best_pos {
                best_pos = abs_pos;
                best_kw = Some(kw);
            }
            start = abs_pos + 1;
        }
    }
    best_kw
}

// ─────────────────────────────────────────────────────────────────────────────
// Method-dot detection
// ─────────────────────────────────────────────────────────────────────────────

/// If the text at `offset-1` is `.` and before it is an identifier/expression,
/// return the text of that expression.
fn detect_method_dot(src: &str, offset: usize) -> Option<String> {
    let bytes = src.as_bytes();
    if offset == 0 {
        return None;
    }
    // Find the character just before the cursor.
    // Walk back to skip the dot.
    let before_cursor = &src[..offset];
    let trimmed = before_cursor.trim_end();
    if !trimmed.ends_with('.') {
        return None;
    }
    // Extract the expression/identifier before the dot.
    let before_dot = &trimmed[..trimmed.len() - 1];
    let obj_text = extract_last_expr(before_dot);
    if obj_text.is_empty() {
        return None;
    }
    // Don't treat decimal numbers like `3.` as method calls.
    if obj_text.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let _ = bytes; // suppress unused
    Some(obj_text)
}

/// Extract the last simple expression from `text` (trailing identifier or call).
fn extract_last_expr(text: &str) -> String {
    let text = text.trim_end();
    if text.is_empty() {
        return String::new();
    }
    // If ends with `)` or `]`, take everything up to and including it.
    // For simplicity, we return the last identifier (most common case for V1).
    let bytes = text.as_bytes();
    let end = bytes.len();
    let mut i = end;
    // Walk back over identifier chars.
    while i > 0
        && (bytes[i - 1].is_ascii_alphanumeric()
            || bytes[i - 1] == b'_')
    {
        i -= 1;
    }
    let ident = &text[i..end];
    // If preceded by another dot, return the full chain up to here.
    ident.to_string()
}

// ─────────────────────────────────────────────────────────────────────────────
// Comment / string detection
// ─────────────────────────────────────────────────────────────────────────────

/// True if `offset` is inside a line comment (`//...`).
pub fn is_in_comment(src: &str, offset: usize) -> bool {
    let line_start = find_line_start(src, offset);
    let line = &src[line_start..offset];
    // Check for `//` outside strings on this line.
    let mut in_str = false;
    let mut in_str_char = b'"';
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if !in_str {
            if bytes[i] == b'"' || bytes[i] == b'\'' {
                in_str = true;
                in_str_char = bytes[i];
                i += 1;
                continue;
            }
            if bytes[i] == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                return true;
            }
        } else if bytes[i] == in_str_char && (i == 0 || bytes[i - 1] != b'\\') {
            in_str = false;
        }
        i += 1;
    }
    false
}

/// True if `offset` is inside a string literal.
pub fn is_in_string(src: &str, offset: usize) -> bool {
    let bytes = src.as_bytes();
    let end = offset.min(bytes.len());
    let mut in_str = false;
    let mut escaped = false;
    let mut i = 0;
    while i < end {
        let b = bytes[i];
        if escaped {
            escaped = false;
        } else if b == b'\\' && in_str {
            escaped = true;
        } else if b == b'"' {
            in_str = !in_str;
        } else if b == b'\n' {
            // Nova strings don't span lines (V1 assumption).
            in_str = false;
        }
        i += 1;
    }
    in_str
}

/// Find the byte offset of the start of the line containing `offset`.
fn find_line_start(src: &str, offset: usize) -> usize {
    let end = offset.min(src.len());
    let bytes = src.as_bytes();
    let mut i = end;
    while i > 0 && bytes[i - 1] != b'\n' {
        i -= 1;
    }
    i
}

// ─────────────────────────────────────────────────────────────────────────────
// Keyword completion (104.3.1)
// ─────────────────────────────────────────────────────────────────────────────

/// Keywords available at the top level.
const TOP_LEVEL_KEYWORDS: &[(&str, &str)] = &[
    ("fn", "Declare a function"),
    ("type", "Declare a type (record, sum, alias, ...)"),
    ("const", "Declare a compile-time constant"),
    ("import", "Import a module or items"),
    ("export", "Export a declaration"),
    ("module", "Module declaration"),
    ("effect", "Declare an effect type"),
    ("protocol", "Declare a protocol"),
    ("test", "Declare a test block"),
    ("bench", "Declare a benchmark"),
    ("lemma", "Declare a proven lemma"),
    ("extern", "Declare an extern fn (extern \"C\" fn ...)"),
    ("priv", "Mark type fields as module-private"),
];

/// Keywords available inside a function body.
const FN_BODY_KEYWORDS: &[(&str, &str)] = &[
    ("ro", "Readonly binding (default)"),
    ("mut", "Mutable binding"),
    ("if", "Conditional expression"),
    ("else", "Else branch"),
    ("for", "Iteration loop"),
    ("while", "While loop"),
    ("return", "Return from function"),
    ("match", "Pattern match expression"),
    ("break", "Break from loop"),
    ("continue", "Continue loop iteration"),
    ("fn", "Nested function declaration"),
    ("type", "Local type alias"),
    ("effect", "Effect declaration"),
    ("defer", "Defer expression to function exit"),
    ("unsafe", "Unsafe block"),
    ("uninit", "Possibly-uninit type modifier (uninit T / *uninit T)"),
    ("consume", "Consume a value"),
    ("apply", "Apply a lemma"),
    ("reveal", "Reveal an opaque function body"),
];

/// Keywords available inside a type body.
const TYPE_BODY_KEYWORDS: &[(&str, &str)] = &[
    ("fn", "Declare a method"),
    ("const", "Declare an associated constant"),
    ("type", "Declare an associated type alias"),
    ("pub", "Mark a field as public"),
    ("priv", "Mark a field as private"),
    ("value", "Mark type as stack-allocated value type"),
];

/// Context tag stored in a keyword item's `data` so [`resolve_completion_item`]
/// can look the exact doc back up (the same label carries a different doc in the
/// top-level vs fn-body vs type-body table).
fn keyword_ctx_tag(ctx: &CompletionContext) -> &'static str {
    match ctx {
        CompletionContext::TopLevel => "top",
        CompletionContext::TypeBody => "type",
        _ => "fn",
    }
}

/// Build keyword completion items for the given context.
///
/// Ф.13: `detail` and `documentation` are DEFERRED — they are re-derived from
/// the keyword tables in [`resolve_completion_item`] via the `data` descriptor.
pub fn keyword_items(ctx: &CompletionContext) -> Vec<CompletionItem> {
    let kws: &[(&str, &str)] = match ctx {
        CompletionContext::TopLevel => TOP_LEVEL_KEYWORDS,
        CompletionContext::FnBody => FN_BODY_KEYWORDS,
        CompletionContext::TypeBody => TYPE_BODY_KEYWORDS,
        CompletionContext::Import { .. } | CompletionContext::MethodDot { .. } => return vec![],
    };
    let tag = keyword_ctx_tag(ctx);

    kws.iter()
        .map(|(kw, _doc)| CompletionItem {
            label: kw.to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            sort_text: Some(format!("{}{}", RANK_KEYWORD, kw)),
            data: Some(serde_json::json!({ "f": "kw", "k": kw, "c": tag })),
            ..Default::default()
        })
        .collect()
}

// ─────────────────────────────────────────────────────────────────────────────
// Snippet completion (104.3.1)
// ─────────────────────────────────────────────────────────────────────────────

/// A snippet definition.
struct Snippet {
    label: &'static str,
    detail: &'static str,
    insert_text: &'static str,
    context: SnippetContext,
}

#[derive(PartialEq, Eq)]
enum SnippetContext {
    TopLevel,
    FnBody,
    Both,
}

const SNIPPETS: &[Snippet] = &[
    Snippet {
        label: "fn",
        detail: "fn name(params) -> RetTy { ... }",
        insert_text: "fn ${1:name}(${2:params}) -> ${3:RetTy} {\n\t${4:()}\n}",
        context: SnippetContext::Both,
    },
    Snippet {
        label: "type",
        detail: "type Name { field Type }",
        insert_text: "type ${1:Name} {\n\t${2:field} ${3:Type}\n}",
        context: SnippetContext::TopLevel,
    },
    Snippet {
        label: "match",
        detail: "match expr { pattern => () }",
        insert_text: "match ${1:expr} {\n\t${2:pattern} => ${3:()}\n}",
        context: SnippetContext::FnBody,
    },
    Snippet {
        label: "if",
        detail: "if cond { ... } else { ... }",
        insert_text: "if ${1:cond} {\n\t${2:()}\n} else {\n\t${3:()}\n}",
        context: SnippetContext::FnBody,
    },
    Snippet {
        label: "if-let",
        detail: "if let Some(x) = expr { ... }",
        insert_text: "if let Some(${1:x}) = ${2:expr} {\n\t${3:()}\n}",
        context: SnippetContext::FnBody,
    },
    Snippet {
        label: "while-let",
        detail: "while let Some(x) = expr { ... }",
        insert_text: "while let Some(${1:x}) = ${2:expr} {\n\t${3:()}\n}",
        context: SnippetContext::FnBody,
    },
    Snippet {
        label: "for",
        detail: "for item in iterable { ... }",
        insert_text: "for ${1:item} in ${2:iterable} {\n\t${3:()}\n}",
        context: SnippetContext::FnBody,
    },
    Snippet {
        label: "while",
        detail: "while cond { ... }",
        insert_text: "while ${1:true} {\n\t${2:()}\n}",
        context: SnippetContext::FnBody,
    },
    Snippet {
        label: "test",
        detail: "test \"name\" { ... }",
        insert_text: "test \"${1:name}\" {\n\t${2:()}\n}",
        context: SnippetContext::TopLevel,
    },
    Snippet {
        label: "defer",
        detail: "defer { ... }",
        insert_text: "defer {\n\t${1:()}\n}",
        context: SnippetContext::FnBody,
    },
];

/// Build snippet completion items for the given context.
pub fn snippet_items(ctx: &CompletionContext) -> Vec<CompletionItem> {
    let (is_top, is_fn) = match ctx {
        CompletionContext::TopLevel => (true, false),
        CompletionContext::FnBody => (false, true),
        _ => return vec![],
    };

    SNIPPETS
        .iter()
        .filter(|s| match s.context {
            SnippetContext::TopLevel => is_top,
            SnippetContext::FnBody => is_fn,
            SnippetContext::Both => is_top || is_fn,
        })
        // Ф.13: `detail`/`documentation` deferred to resolve (re-derived from
        // `SNIPPETS` by label). `insert_text` is kept inline — it is required to
        // apply the snippet even if the client commits without resolving.
        .map(|s| CompletionItem {
            label: s.label.to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            insert_text: Some(s.insert_text.to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            sort_text: Some(format!("{}{}", RANK_SNIPPET, s.label)),
            data: Some(serde_json::json!({ "f": "snip", "k": s.label })),
            ..Default::default()
        })
        .collect()
}

// ─────────────────────────────────────────────────────────────────────────────
// In-scope identifier completion (104.3.2)
// ─────────────────────────────────────────────────────────────────────────────

/// Identifiers from parsing that we surface as completions.
#[derive(Debug, Clone)]
pub struct IdentInfo {
    pub name: String,
    pub kind: IdentKind,
    pub type_hint: Option<String>,
    pub rank: &'static str, // sort_text prefix
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdentKind {
    Local,  // let binding / fn param
    Fn,     // free function
    Type,   // type decl
    Const,  // const decl
    Prelude, // built-in prelude symbol
}

/// Scan `src` up to `offset` for in-scope identifiers.
///
/// This is a text-level scan (not full AST walk):
/// - Extract `fn`, `type`, `const`, `let`, `mut`, param names in scope.
/// - Avoids re-implementing the full type-checker.
/// `offset` is clamped to `src.len()` if past the end.
pub fn collect_scope_identifiers(src: &str, offset: usize) -> Vec<IdentInfo> {
    let end = offset.min(src.len());
    let text = &src[..end];
    let mut idents: Vec<IdentInfo> = Vec::new();

    // Collect locals (let/mut bindings) from fn body scan.
    idents.extend(collect_let_bindings(text));
    // Collect fn parameters from enclosing fn signature.
    idents.extend(collect_fn_params(src, offset));
    // Collect top-level declarations.
    idents.extend(collect_top_level_decls(src));
    // Add prelude.
    idents.extend(prelude_items());

    // Deduplicate by name (locals win due to rank ordering).
    let mut seen = std::collections::HashSet::new();
    idents.retain(|i| seen.insert(i.name.clone()));
    idents
}

/// Scan for `ro X ...` and `mut X ...` bindings before `offset`.
/// Nova uses `ro` (default) and `mut` for bindings; `let` was removed in Plan 114.
fn collect_let_bindings(text: &str) -> Vec<IdentInfo> {
    let mut out = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed
            .strip_prefix("ro ")
            .or_else(|| trimmed.strip_prefix("mut "))
            // keep compatibility with old `let` (compiler emits E_KW_REMOVED_LET, but
            // the LSP should still not crash if a user types it mid-edit)
            .or_else(|| trimmed.strip_prefix("let "))
        {
            // Name is the first identifier.
            let name = first_ident(rest);
            if !name.is_empty() {
                // Try to extract type hint: `ro name TYPE = ...`
                let type_hint = extract_type_after_name(rest, &name);
                out.push(IdentInfo {
                    name,
                    kind: IdentKind::Local,
                    type_hint,
                    rank: RANK_LOCAL,
                });
            }
        }
    }
    out
}

/// Extract the first identifier from `s`.
fn first_ident(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    let start = i;
    while i < bytes.len()
        && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_')
    {
        i += 1;
    }
    s[start..i].to_string()
}

/// Try to extract the type name that follows `name` in a `let name TYPE = ...` pattern.
fn extract_type_after_name(text: &str, name: &str) -> Option<String> {
    let after_name = text.find(name.as_bytes()[0] as char).and_then(|pos| {
        let tail = &text[pos + name.len()..];
        Some(tail)
    })?;
    let trimmed = after_name.trim_start();
    // Skip if starts with `=` (no type annotation).
    if trimmed.starts_with('=') {
        return None;
    }
    // The type is the first token.
    let ty = first_ident(trimmed);
    if ty.is_empty() || ty == "=" {
        None
    } else {
        Some(ty)
    }
}

/// Extract the declared type of a binding by scanning annotation in `src`.
/// Only inspects explicit `ro`/`mut` annotations — no naming-convention guessing.
fn extract_binding_type(name: &str, src: &str) -> Option<String> {
    for line in src.lines() {
        let t = line.trim();
        for prefix in &["ro ", "mut "] {
            if let Some(rest) = t.strip_prefix(prefix) {
                if first_ident(rest) == name {
                    if let Some(ty) = extract_type_after_name(rest, name) {
                        return Some(ty);
                    }
                }
            }
        }
        if t.contains('(') && t.contains(name) {
            if let Some(paren) = t.find('(') {
                for param in t[paren + 1..].split(',') {
                    let p = param.trim();
                    let p = p.strip_prefix("ro ").or_else(|| p.strip_prefix("mut ")).unwrap_or(p);
                    if first_ident(p) == name {
                        if let Some(ty) = extract_type_after_name(p, name) {
                            return Some(ty);
                        }
                    }
                }
            }
        }
    }
    None
}

/// Scan for `fn name(params, ...)` before `offset` and return param names.
fn collect_fn_params(src: &str, offset: usize) -> Vec<IdentInfo> {
    let end = offset.min(src.len());
    let text = &src[..end];
    let mut out = Vec::new();

    // Find the last `fn` keyword before offset.
    let mut last_fn_pos = None;
    let mut start = 0;
    while let Some(pos) = text[start..].find("fn ") {
        let abs = start + pos;
        // Word boundary check.
        if abs == 0 || !text.as_bytes()[abs - 1].is_ascii_alphanumeric() {
            last_fn_pos = Some(abs);
        }
        start = abs + 1;
    }
    let fn_pos = match last_fn_pos {
        Some(p) => p,
        None => return out,
    };

    // Extract the parameter list from `fn name(...)`.
    let after_fn = &text[fn_pos..];
    let paren_start = match after_fn.find('(') {
        Some(p) => p,
        None => return out,
    };
    let params_text = &after_fn[paren_start + 1..];
    let paren_end = match params_text.find(')') {
        Some(p) => p,
        None => params_text.len(),
    };
    let params_str = &params_text[..paren_end];
    // Parse `name Type, name Type, ...` — params may be prefixed with `ro`/`mut`.
    for param in params_str.split(',') {
        let p = param.trim();
        // Strip binding-modifier prefix so we extract the actual name.
        let p = p.strip_prefix("ro ").or_else(|| p.strip_prefix("mut ")).unwrap_or(p);
        let name = first_ident(p);
        if name.is_empty() || name == "self" {
            continue;
        }
        let type_hint = extract_type_after_name(p, &name);
        out.push(IdentInfo {
            name,
            kind: IdentKind::Local,
            type_hint,
            rank: RANK_LOCAL,
        });
    }
    out
}

/// Scan the whole `src` for top-level `fn`, `type`, `const` declarations.
fn collect_top_level_decls(src: &str) -> Vec<IdentInfo> {
    let mut out = Vec::new();
    for line in src.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("fn ").or_else(|| t.strip_prefix("pub fn ")) {
            // Skip receiver methods (contain `@`).
            if rest.contains('@') {
                // Receiver method: extract method name after `@`.
                continue;
            }
            let name = first_ident(rest);
            if !name.is_empty() {
                out.push(IdentInfo {
                    name,
                    kind: IdentKind::Fn,
                    type_hint: None,
                    rank: RANK_MODULE,
                });
            }
        } else if let Some(rest) = t
            .strip_prefix("type ")
            .or_else(|| t.strip_prefix("pub type "))
            .or_else(|| t.strip_prefix("export type "))
        {
            let name = first_ident(rest);
            if !name.is_empty() {
                out.push(IdentInfo {
                    name,
                    kind: IdentKind::Type,
                    type_hint: None,
                    rank: RANK_MODULE,
                });
            }
        } else if let Some(rest) = t
            .strip_prefix("const ")
            .or_else(|| t.strip_prefix("pub const "))
        {
            let name = first_ident(rest);
            if !name.is_empty() {
                out.push(IdentInfo {
                    name,
                    kind: IdentKind::Const,
                    type_hint: None,
                    rank: RANK_MODULE,
                });
            }
        }
    }
    out
}

/// Hardcoded Nova prelude items — lifted to module scope so both
/// [`prelude_items`] (initial list) and [`resolve_completion_item`] (lazy doc)
/// share one source of truth (Ф.13).
const PRELUDE_TABLE: &[(&str, &str, IdentKind)] = &[
        // Primitive types (Plan 133: usize/isize removed; float literal infers f64).
        ("int", "64-bit signed integer (Nova's universal integer type)", IdentKind::Type),
        ("f64", "64-bit floating-point number", IdentKind::Type),
        ("f32", "32-bit floating-point number", IdentKind::Type),
        ("bool", "boolean (true / false)", IdentKind::Type),
        ("str", "UTF-8 string value type", IdentKind::Type),
        ("char", "Unicode scalar value (U+0000..U+D7FF, U+E000..U+10FFFF)", IdentKind::Type),
        ("u8", "unsigned 8-bit integer", IdentKind::Type),
        ("u16", "unsigned 16-bit integer", IdentKind::Type),
        ("u32", "unsigned 32-bit integer", IdentKind::Type),
        ("u64", "unsigned 64-bit integer", IdentKind::Type),
        ("i8", "signed 8-bit integer", IdentKind::Type),
        ("i16", "signed 16-bit integer", IdentKind::Type),
        ("i32", "signed 32-bit integer", IdentKind::Type),
        ("i64", "signed 64-bit integer (alias of int)", IdentKind::Type),
        // Stdlib types from prelude (std/prelude/core.nv, std/prelude/collections.nv).
        ("Option", "Option[T] — Some(T) | None", IdentKind::Type),
        ("Result", "Result[T, E] — Ok(T) | Err(E)", IdentKind::Type),
        ("Vec", "Vec[T] — owned growable array ([]T is a sugar alias)", IdentKind::Type),
        ("HashMap", "HashMap[K, V] — hash map (import std.collections.hashmap)", IdentKind::Type),
        ("Set", "Set[T] — hash set (import std.collections.set)", IdentKind::Type),
        ("Range", "Range — integer range a..b", IdentKind::Type),
        ("StringBuilder", "mutable UTF-8 string builder", IdentKind::Type),
        // Protocols (std/prelude/protocols.nv)
        ("Compare", "protocol Compare — three-way comparison via @compare (Plan 91.8a, D183)", IdentKind::Type),
        ("Debug", "protocol Debug — debug output via @debug; used by ${x:?} interpolation (Plan 91.14, D229)", IdentKind::Type),
        // Common prelude functions/values.
        ("print", "print(s str) — write to stdout without newline", IdentKind::Fn),
        ("println", "println(s str) — write to stdout with newline", IdentKind::Fn),
        ("assert", "assert(cond bool, msg str) — runtime assertion", IdentKind::Fn),
        ("panic", "panic(msg str) — abort with message", IdentKind::Fn),
        ("todo", "todo() — panic with 'not yet implemented'", IdentKind::Fn),
        // Boolean literals.
        ("true", "boolean true", IdentKind::Const),
        ("false", "boolean false", IdentKind::Const),
    ];

/// Hardcoded Nova prelude items surfaced as identifier completions.
fn prelude_items() -> Vec<IdentInfo> {
    PRELUDE_TABLE
        .iter()
        .map(|(name, doc, kind)| IdentInfo {
            name: name.to_string(),
            kind: kind.clone(),
            type_hint: Some(doc.to_string()),
            rank: RANK_PRELUDE,
        })
        .collect()
}

/// Convert `IdentInfo` to a `CompletionItem`.
///
/// Ф.13: prelude entries carry a verbose one-line doc in `type_hint`; that
/// `detail` is DEFERRED and re-derived from the prelude table on resolve. Local
/// / module identifiers keep their short type-name `detail` inline (cheap, and
/// useful when the client does not resolve).
pub fn ident_info_to_item(info: &IdentInfo) -> CompletionItem {
    let kind = match info.kind {
        IdentKind::Local => CompletionItemKind::VARIABLE,
        IdentKind::Fn => CompletionItemKind::FUNCTION,
        IdentKind::Type => CompletionItemKind::CLASS,
        IdentKind::Const => CompletionItemKind::CONSTANT,
        IdentKind::Prelude => CompletionItemKind::KEYWORD,
    };
    // Prelude items are recognised by their rank; their doc is deferred.
    if info.rank == RANK_PRELUDE {
        return CompletionItem {
            label: info.name.clone(),
            kind: Some(kind),
            sort_text: Some(format!("{}{}", info.rank, info.name)),
            data: Some(serde_json::json!({ "f": "prelude", "k": info.name })),
            ..Default::default()
        };
    }
    CompletionItem {
        label: info.name.clone(),
        kind: Some(kind),
        detail: info.type_hint.clone(),
        sort_text: Some(format!("{}{}", info.rank, info.name)),
        ..Default::default()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Method-dot completion (104.3.3)
// ─────────────────────────────────────────────────────────────────────────────



/// Byte offset of the method-dot `.` immediately before `offset` (after
/// trimming trailing whitespace), or `None` if the cursor is not a method-dot
/// position. Rejects a purely-numeric receiver (`3.` is a float, not a call).
fn method_dot_offset(src: &str, offset: usize) -> Option<usize> {
    let before = &src[..offset.min(src.len())];
    let trimmed = before.trim_end();
    if !trimmed.ends_with('.') {
        return None;
    }
    let dot_byte = trimmed.len() - 1; // '.' is ASCII (1 byte)
    let before_dot = trimmed[..dot_byte].trim_end();
    // A method dot requires an expression before it. Accept identifiers, call
    // results (`foo()`), and index results (`a[i]`); reject an empty receiver and
    // a bare numeric literal (`3.` is a float, not a member access).
    match before_dot.chars().last() {
        None => None,
        Some(')') | Some(']') => Some(dot_byte),
        Some(c) if c.is_ascii_alphanumeric() || c == '_' => {
            let obj = extract_last_expr(before_dot);
            if obj.is_empty() || obj.chars().all(|c| c.is_ascii_digit()) {
                None
            } else {
                Some(dot_byte)
            }
        }
        // Anything else before the dot (operator, `.`, etc.) — not a receiver we
        // can complete on.
        Some(_) => None,
    }
}

/// Repair a completion buffer so it parses, WITHOUT shifting any byte offset at
/// or before `dot_byte` (so the receiver expression still ends exactly there):
///
/// 1. Overwrite the dangling method-dot at `dot_byte` with a space (`.` and ` `
///    are both one byte) — turns the unparseable `recv.` into a bare `recv `
///    expression statement whose inferred type the checker records.
/// 2. Append the closing brackets needed to balance any still-open `(`/`[`/`{`
///    (in correct reverse order). Interactive buffers are almost always
///    truncated mid-block at the cursor, which would otherwise be a hard parse
///    error and yield no `expr_types`.
///
/// All appended text lands strictly AFTER the original bytes, so offsets ≤
/// `dot_byte` are preserved. Any residual type errors (e.g. a bare `count` in a
/// `-> ()` function) are tolerated by the lenient IDE checker.
fn repair_completion_buffer(src: &str, dot_byte: usize) -> String {
    let mut s = src.to_string();
    s.replace_range(dot_byte..dot_byte + 1, " ");

    // Balance brackets over the repaired text, skipping string/char literals and
    // line comments so their contents never miscount.
    let bytes = s.as_bytes();
    let mut stack: Vec<u8> = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'/' => {
                // Line comment — skip to end of line.
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
                continue;
            }
            b'"' | b'\'' => {
                let quote = bytes[i];
                i += 1;
                while i < bytes.len() {
                    if bytes[i] == b'\\' {
                        i += 2;
                        continue;
                    }
                    if bytes[i] == quote || bytes[i] == b'\n' {
                        i += 1;
                        break;
                    }
                    i += 1;
                }
                continue;
            }
            b'(' | b'[' | b'{' => stack.push(bytes[i]),
            b')' => {
                if stack.last() == Some(&b'(') {
                    stack.pop();
                }
            }
            b']' => {
                if stack.last() == Some(&b'[') {
                    stack.pop();
                }
            }
            b'}' => {
                if stack.last() == Some(&b'{') {
                    stack.pop();
                }
            }
            _ => {}
        }
        i += 1;
    }
    // Append closers in reverse (innermost first).
    for &opener in stack.iter().rev() {
        s.push(match opener {
            b'(' => ')',
            b'[' => ']',
            _ => '}',
        });
    }
    s
}

/// Extract the base type NAME from an inferred `TypeRef` (the name we look up in
/// the method table). `Named{path:[..,"Foo"]}` → `Foo`; slice/array → `Vec`
/// (the `[]T` sugar aliases `Vec[T]`); modifiers are peeled.
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

/// Find the inferred type-name of the receiver expression that ends exactly at
/// `dot_byte` in the entry file, via the Ф.2 `expr_types` map. When several
/// expressions share that end offset (nested), the OUTERMOST (smallest start)
/// is the full receiver.
pub(crate) fn receiver_type_name(env: &ModuleEnv, dot_byte: usize) -> Option<String> {
    let mut best_start = usize::MAX;
    let mut best_ty: Option<&TypeRef> = None;
    for (span, ty) in &env.expr_types {
        if span.file_id == MAIN_FILE_ID && span.end == dot_byte && span.start < best_start {
            best_start = span.start;
            best_ty = Some(ty);
        }
    }
    best_ty.and_then(type_ref_base_name)
}

/// True if `recv`'s receiver type matches the target type name `ty_name`,
/// accounting for the `[]T`/`Vec[T]` slice-alias equivalence.
pub(crate) fn receiver_matches(recv: &nova_codegen::ast::Receiver, ty_name: &str) -> bool {
    if recv.type_name == ty_name {
        return true;
    }
    // `[]T` receivers (`fn []T @m`) are methods on `Vec[T]`.
    if ty_name == "Vec" && recv.type_name.starts_with("[]") {
        return true;
    }
    if recv.type_name == "Vec" && ty_name.starts_with("[]") {
        return true;
    }
    false
}

/// Build a METHOD completion item for a receiver method declaration.
fn method_completion_item(
    fd: &nova_codegen::ast::FnDecl,
    recv: &nova_codegen::ast::Receiver,
) -> CompletionItem {
    // Ф.13: the signature `detail` stays inline (cheap, shown in the list); the
    // doc-comment `documentation` is deferred — stashed in `data` and moved into
    // `documentation` on resolve ([M-104.10-lsp-resolve-method-doc]).
    let data = crate::symbol::extract_doc(&fd.doc).map(|d| {
        serde_json::json!({ "f": "method", "doc": d })
    });
    CompletionItem {
        label: fd.name.clone(),
        kind: Some(CompletionItemKind::METHOD),
        detail: Some(crate::symbol::format_method_signature(fd, recv)),
        sort_text: Some(format!("{}{}", RANK_MODULE, fd.name)),
        data,
        ..Default::default()
    }
}

/// All instance methods declared on `ty_name` across the resolved module —
/// includes stdlib methods (imported items are inlined into `module.items`) and
/// cross-file peer methods. Static (`.`) methods are excluded: a value receiver
/// `x.` calls instance (`@`) methods only.
fn methods_for_type_name(module: &Module, ty_name: &str) -> Vec<CompletionItem> {
    use nova_codegen::ast::ReceiverKind;
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for item in &module.items {
        if let Item::Fn(fd) = item {
            if let Some(recv) = &fd.receiver {
                if recv.kind == ReceiverKind::Instance
                    && receiver_matches(recv, ty_name)
                    && seen.insert(fd.name.clone())
                {
                    out.push(method_completion_item(fd, recv));
                }
            }
        }
    }
    out
}

/// Type-driven method completions from an already-resolved module. Returns
/// `None` when the receiver type could not be inferred (so the caller can fall
/// back to a text scan); `Some(items)` — possibly empty — when it was resolved.
fn method_items_from_resolved(resolved: &ResolvedModule, dot_byte: usize) -> Option<Vec<CompletionItem>> {
    let env = resolved.env.as_ref()?;
    let ty_name = receiver_type_name(env, dot_byte)?;
    Some(methods_for_type_name(&resolved.module, &ty_name))
}

/// Text-only fallback method scan (no type resolution available): scan the
/// current file's own `fn Type @method` declarations. If the receiver's type is
/// known from a `ro/mut name Type` annotation, filter to it; otherwise show all
/// declared methods (graceful).
fn method_items_text_fallback(src: &str, dot_byte: usize) -> Vec<CompletionItem> {
    let obj = extract_last_expr(&src[..dot_byte]);
    // A non-identifier receiver (`foo()`, `a[i]`) has no textual binding name to
    // look up; show all declared methods (graceful) rather than crash.
    let ty = if obj.is_empty() {
        String::new()
    } else {
        extract_binding_type(&obj, src).unwrap_or_default()
    };
    scan_module_methods(src, &obj, &ty)
}

/// Plan 104.10 Ф.5: type-driven method completion for `expr.│`, given the
/// on-disk `path` of the document (needed to inline stdlib/peer methods and to
/// resolve the receiver's type). Builds a repaired, import-resolved module and
/// looks the receiver type up in `expr_types`; degrades to a text scan when the
/// type cannot be inferred.
pub fn method_items_typed(path: &Path, src: &str, offset: usize) -> Vec<CompletionItem> {
    let Some(dot_byte) = method_dot_offset(src, offset) else {
        return vec![];
    };
    let repaired = repair_completion_buffer(src, dot_byte);
    let resolved = provenance::resolve_module_for_ide(path, &repaired);
    method_items_from_resolved(&resolved, dot_byte)
        .unwrap_or_else(|| method_items_text_fallback(src, dot_byte))
}

/// Compute method completions for `expr.│` at the given offset.
///
/// Path-free convenience wrapper (used by unit/integration tests): discovers the
/// repo root from the current working directory to resolve stdlib/peer methods,
/// then delegates to [`method_items_typed`]. In the LSP server the document's
/// real path is known, so the handler calls [`method_items_typed`] directly —
/// this wrapper's CWD discovery is a test-only best-effort
/// ([M-104.10-lsp-cwd-anchor]).
pub fn method_items(src: &str, offset: usize) -> Vec<CompletionItem> {
    match discover_anchor_path() {
        Some(path) => method_items_typed(&path, src, offset),
        None => match method_dot_offset(src, offset) {
            Some(dot_byte) => method_items_text_fallback(src, dot_byte),
            None => vec![],
        },
    }
}

/// [M-104.10-lsp-cwd-anchor] Best-effort discovery of an existing on-disk `.nv`
/// anchor file inside the current repo, used ONLY by the path-free
/// `completion_for` / `method_items` convenience wrappers so that tests (and any
/// caller lacking a document path) can still resolve stdlib symbols. The LSP
/// server never relies on this — it always has the real document path / a cached
/// [`StdlibIndex`].
///
/// A real *file* (not a directory) is required so `resolve_imports_inline`'s
/// repo-root / stdlib resolution succeeds; the stdlib `prelude.nv` is a stable
/// choice that exists in every Nova workspace. The anchor only supplies the
/// workspace location for import resolution — the completion buffer's own source
/// is what actually gets parsed and type-checked.
fn discover_anchor_path() -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok()?;
    let repo = nova_codegen::test_runner::find_repo_root_from(&cwd)?;
    let anchor = nova_codegen::manifest::resolve_std_path(&repo).join("prelude.nv");
    anchor.exists().then_some(anchor)
}

/// [M-104.10-lsp-cwd-anchor] Best-effort [`StdlibIndex`] for the path-free
/// wrappers: discover the repo root from CWD, resolve its stdlib dir, and build
/// an index. `None` when no workspace is reachable.
fn discover_stdlib_index() -> Option<StdlibIndex> {
    let cwd = std::env::current_dir().ok()?;
    let repo = nova_codegen::test_runner::find_repo_root_from(&cwd)?;
    let stdlib_dir = nova_codegen::manifest::resolve_std_path(&repo);
    Some(StdlibIndex::build(&stdlib_dir, "std"))
}


/// Scan src for `fn TypeName @method_name(...)` declarations.
/// If `ty` is known, only returns methods for that type.
/// If `ty` is empty (unknown receiver), returns all methods declared in the file.
fn scan_module_methods(src: &str, _obj_text: &str, ty: &str) -> Vec<CompletionItem> {
    let mut out = Vec::new();
    for line in src.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("fn ") {
            // Pattern: `fn TYPE @method_name(...)`
            let type_name = first_ident(rest);
            if type_name.is_empty() || type_name == "fn" {
                continue;
            }
            let after_type = rest[type_name.len()..].trim_start();
            if let Some(rest2) = after_type.strip_prefix('@') {
                if !ty.is_empty() && type_name != ty {
                    continue;
                }
                let name = first_ident(rest2);
                if !name.is_empty() {
                    out.push(CompletionItem {
                        label: name.clone(),
                        kind: Some(CompletionItemKind::METHOD),
                        detail: Some(if ty.is_empty() {
                            format!("method on {}", type_name)
                        } else {
                            format!("method on {}", ty)
                        }),
                        sort_text: Some(format!("{}{}", RANK_MODULE, name)),
                        ..Default::default()
                    });
                }
            }
        }
    }
    out
}

// ─────────────────────────────────────────────────────────────────────────────
// Import path completion (104.3.4 + Ф.5 [M-104.10-hardcode-lists])
// ─────────────────────────────────────────────────────────────────────────────

/// Build import-path completions for `path_prefix` from a filesystem-derived
/// [`StdlibIndex`] (no hardcoded module list — compiler-conventions §3). Each
/// suggested segment is a module that actually exists on disk under the given
/// prefix.
pub fn import_items_from_index(idx: &StdlibIndex, path_prefix: &[String]) -> Vec<CompletionItem> {
    let prefix_str = path_prefix.join(".");
    idx.child_segments(path_prefix)
        .into_iter()
        .map(|segment| {
            let full = if prefix_str.is_empty() {
                segment.clone()
            } else {
                format!("{}.{}", prefix_str, segment)
            };
            // Ф.13: `documentation` deferred — re-derived from the full path in
            // `data` on resolve. `detail` stays inline (short, useful).
            CompletionItem {
                label: segment.clone(),
                kind: Some(CompletionItemKind::MODULE),
                detail: Some(format!("module {}", full)),
                sort_text: Some(format!("{}{}", RANK_STD, segment)),
                data: Some(serde_json::json!({ "f": "import", "k": full })),
                ..Default::default()
            }
        })
        .collect()
}

/// Build import path completions for a given path prefix.
///
/// Path-free convenience wrapper (tests / callers without a resolved
/// [`StdlibIndex`]): discovers the stdlib dir from the current working directory
/// ([M-104.10-lsp-cwd-anchor]) and builds an index on the fly. The LSP server
/// uses a cached index and calls [`import_items_from_index`] directly.
pub fn import_items(path_prefix: &[String]) -> Vec<CompletionItem> {
    match discover_stdlib_index() {
        Some(idx) => import_items_from_index(&idx, path_prefix),
        None => vec![],
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Main entry point
// ─────────────────────────────────────────────────────────────────────────────

/// Core completion computation. `path` (document location) enables type-driven
/// method completion; `stdlib` (a resolved [`StdlibIndex`]) enables import
/// completion. Either may be `None`, in which case that context degrades
/// gracefully (empty / text-only).
fn completion_core(
    src: &str,
    offset: usize,
    path: Option<&Path>,
    stdlib: Option<&StdlibIndex>,
) -> Vec<CompletionItem> {
    let offset = offset.min(src.len());
    let ctx = match detect_context(src, offset) {
        Some(c) => c,
        None => return vec![], // in comment or string
    };

    let mut items: Vec<CompletionItem> = Vec::new();

    match &ctx {
        CompletionContext::MethodDot { .. } => {
            match path {
                Some(p) => items.extend(method_items_typed(p, src, offset)),
                None => {
                    if let Some(dot_byte) = method_dot_offset(src, offset) {
                        items.extend(method_items_text_fallback(src, dot_byte));
                    }
                }
            }
        }
        CompletionContext::Import { path_prefix } => {
            if let Some(idx) = stdlib {
                items.extend(import_items_from_index(idx, path_prefix));
            }
        }
        CompletionContext::TopLevel | CompletionContext::FnBody | CompletionContext::TypeBody => {
            // Keyword + snippet completions.
            items.extend(keyword_items(&ctx));
            items.extend(snippet_items(&ctx));

            // Identifier completions (only in fn body and top level, not type body).
            if matches!(ctx, CompletionContext::FnBody | CompletionContext::TopLevel) {
                let idents = collect_scope_identifiers(src, offset);
                items.extend(idents.iter().map(ident_info_to_item));
            }
        }
    }

    // Deduplicate by label (prefer first occurrence = higher ranked).
    let mut seen_labels = std::collections::HashSet::new();
    items.retain(|i| seen_labels.insert(i.label.clone()));

    items
}

/// Compute all completion items for cursor at `offset` in `src`.
///
/// Path-free convenience entry point (unit/integration tests, and any caller
/// without a document path): stdlib/method resolution is discovered from the
/// current working directory ([M-104.10-lsp-cwd-anchor]). The LSP server calls
/// [`completion_for_doc`] with the real document path + a cached index.
///
/// Returns an empty Vec if the cursor is in a comment or string, or nothing
/// applies. `offset` is clamped to `src.len()`.
pub fn completion_for(src: &str, offset: usize) -> Vec<CompletionItem> {
    let anchor = discover_anchor_path();
    let stdlib = discover_stdlib_index();
    completion_core(src, offset, anchor.as_deref(), stdlib.as_ref())
}

/// Plan 104.10 Ф.5: LSP-server completion entry point. `path` is the document's
/// on-disk location (enables type-driven method completion) and `stdlib` is the
/// workspace's cached [`StdlibIndex`] (enables FS-sourced import completion).
pub fn completion_for_doc(
    path: &Path,
    src: &str,
    offset: usize,
    stdlib: Option<&StdlibIndex>,
) -> Vec<CompletionItem> {
    completion_core(src, offset, Some(path), stdlib)
}

// ─────────────────────────────────────────────────────────────────────────────
// Lazy resolve (Plan 104.10 Ф.13)
// ─────────────────────────────────────────────────────────────────────────────

/// Look up a keyword's short doc for the given context tag (`"top"`/`"fn"`/
/// `"type"`), matching the table [`keyword_items`] built the item from.
fn keyword_doc(label: &str, ctx_tag: &str) -> Option<&'static str> {
    let table: &[(&str, &str)] = match ctx_tag {
        "top" => TOP_LEVEL_KEYWORDS,
        "type" => TYPE_BODY_KEYWORDS,
        _ => FN_BODY_KEYWORDS,
    };
    table.iter().find(|(kw, _)| *kw == label).map(|(_, d)| *d)
}

/// Look up a snippet's `detail` by label.
fn snippet_detail(label: &str) -> Option<&'static str> {
    SNIPPETS.iter().find(|s| s.label == label).map(|s| s.detail)
}

/// Look up a prelude entry's one-line doc by name.
fn prelude_doc(name: &str) -> Option<&'static str> {
    PRELUDE_TABLE.iter().find(|(n, _, _)| *n == name).map(|(_, d, _)| *d)
}

/// `completionItem/resolve` — lazily attach the heavy `detail`/`documentation`
/// that the initial list omitted (Ф.13).
///
/// The item's `data` descriptor (`{"f": <family>, ...}`) tells us how to
/// re-derive the fields:
/// - `"kw"`   → keyword table (`k` = label, `c` = context tag)
/// - `"snip"` → snippet table (`k` = label)
/// - `"prelude"` → prelude table (`k` = name)
/// - `"method"` → stashed markdown in `doc`
/// - `"import"` → module path in `k`
///
/// Any item without a recognised `data` payload (locals, text-fallback methods,
/// an already-resolved item, or a malformed/unknown descriptor) is returned
/// unchanged — resolve is always graceful and idempotent.
pub fn resolve_completion_item(mut item: CompletionItem) -> CompletionItem {
    let Some(data) = item.data.clone() else {
        return item;
    };
    let get = |key: &str| data.get(key).and_then(|v| v.as_str());
    match get("f") {
        Some("kw") => {
            if let Some(doc) = get("k").and_then(|k| keyword_doc(k, get("c").unwrap_or("fn"))) {
                item.detail = Some(doc.to_string());
                item.documentation = Some(Documentation::MarkupContent(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: format!("**Nova keyword** — {}", doc),
                }));
            }
        }
        Some("snip") => {
            if let Some(detail) = get("k").and_then(snippet_detail) {
                item.detail = Some(detail.to_string());
                item.documentation = Some(Documentation::MarkupContent(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: format!("**Snippet** — `{}`", detail),
                }));
            }
        }
        Some("prelude") => {
            if let Some(doc) = get("k").and_then(prelude_doc) {
                item.detail = Some(doc.to_string());
                item.documentation = Some(Documentation::MarkupContent(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: doc.to_string(),
                }));
            }
        }
        Some("method") => {
            if let Some(doc) = get("doc") {
                item.documentation = Some(Documentation::MarkupContent(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: doc.to_string(),
                }));
            }
        }
        Some("import") => {
            if let Some(full) = get("k") {
                item.documentation = Some(Documentation::MarkupContent(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: format!("**module** `{}`", full),
                }));
            }
        }
        _ => {}
    }
    item
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Helpers ──────────────────────────────────────────────────────────────

    fn has_label(items: &[CompletionItem], label: &str) -> bool {
        items.iter().any(|i| i.label == label)
    }

    // ─────────────────────────────────────────────────────────────────────────
    // 104.3.1 — Keyword/snippet (5 pos + 2 neg)
    // ─────────────────────────────────────────────────────────────────────────

    /// kw_pos1: top-level keywords include `fn`, `type`, `import`.
    #[test]
    fn kw_pos1_top_level_keywords() {
        let ctx = CompletionContext::TopLevel;
        let items = keyword_items(&ctx);
        assert!(has_label(&items, "fn"), "fn missing from top-level keywords");
        assert!(has_label(&items, "type"), "type missing");
        assert!(has_label(&items, "import"), "import missing");
        assert!(has_label(&items, "const"), "const missing");
        assert!(has_label(&items, "module"), "module missing");
    }

    /// kw_pos2: fn-body keywords include `ro`, `mut`, `if`, `for`, `match`.
    /// Note: `let` was removed in Nova Plan 114 — it is NOT in fn-body keywords.
    #[test]
    fn kw_pos2_fn_body_keywords() {
        let ctx = CompletionContext::FnBody;
        let items = keyword_items(&ctx);
        assert!(has_label(&items, "ro"), "ro missing from fn-body keywords");
        assert!(has_label(&items, "mut"), "mut missing from fn-body keywords");
        assert!(has_label(&items, "if"), "if missing");
        assert!(has_label(&items, "for"), "for missing");
        assert!(has_label(&items, "match"), "match missing");
        assert!(has_label(&items, "return"), "return missing");
        assert!(!has_label(&items, "let"), "let must NOT appear — removed in Plan 114");
    }

    /// kw_pos3: type-body keywords include `fn`, `const`, `pub`.
    #[test]
    fn kw_pos3_type_body_keywords() {
        let ctx = CompletionContext::TypeBody;
        let items = keyword_items(&ctx);
        assert!(has_label(&items, "fn"), "fn missing from type-body keywords");
        assert!(has_label(&items, "pub"), "pub missing");
    }

    /// kw_pos4: keyword items have KEYWORD kind and non-empty sort_text.
    #[test]
    fn kw_pos4_keyword_item_structure() {
        let ctx = CompletionContext::TopLevel;
        let items = keyword_items(&ctx);
        for item in &items {
            assert_eq!(item.kind, Some(CompletionItemKind::KEYWORD));
            assert!(item.sort_text.is_some(), "sort_text must be set");
            assert!(
                item.sort_text.as_deref().unwrap().starts_with(RANK_KEYWORD),
                "sort_text must start with keyword rank prefix"
            );
        }
    }

    /// kw_pos5: snippet items have SNIPPET kind and insert_text with placeholders.
    #[test]
    fn kw_pos5_snippets_have_insert_text() {
        let ctx = CompletionContext::FnBody;
        let items = snippet_items(&ctx);
        assert!(!items.is_empty(), "fn-body should have snippets");
        for item in &items {
            assert_eq!(item.kind, Some(CompletionItemKind::SNIPPET));
            assert!(item.insert_text.is_some(), "snippet must have insert_text");
            let text = item.insert_text.as_deref().unwrap();
            assert!(text.contains("${"), "snippet insert_text must have placeholders");
            assert_eq!(
                item.insert_text_format,
                Some(InsertTextFormat::SNIPPET)
            );
        }
    }

    /// kw_neg1: method-dot context returns no keywords.
    #[test]
    fn kw_neg1_no_keywords_for_method_dot() {
        let ctx = CompletionContext::MethodDot { obj_text: "x".to_string() };
        let items = keyword_items(&ctx);
        assert!(items.is_empty(), "method-dot should return no keywords");
    }

    /// kw_neg2: import context returns no keywords.
    #[test]
    fn kw_neg2_no_keywords_for_import() {
        let ctx = CompletionContext::Import { path_prefix: vec!["std".to_string()] };
        let items = keyword_items(&ctx);
        assert!(items.is_empty(), "import should return no keywords");
    }

    // ─────────────────────────────────────────────────────────────────────────
    // 104.3.2 — In-scope identifier (5 pos + 2 neg)
    // ─────────────────────────────────────────────────────────────────────────

    const IDENT_SRC: &str = r#"
module test.m

fn add(a int, b int) -> int => a + b

fn main() -> () {
    ro x int = 5
    mut y f64 = 3.14
    ro result int = add(x, 1)
}
"#;

    /// id_pos1: let bindings appear as LOCAL identifiers.
    #[test]
    fn id_pos1_let_bindings_appear() {
        let offset = IDENT_SRC.find("add(x, 1)").unwrap();
        let idents = collect_scope_identifiers(IDENT_SRC, offset);
        assert!(
            idents.iter().any(|i| i.name == "x" && i.kind == IdentKind::Local),
            "x should be a local"
        );
    }

    /// id_pos2: fn params appear as LOCAL identifiers.
    #[test]
    fn id_pos2_fn_params_appear() {
        let offset = IDENT_SRC.len();
        let idents = collect_scope_identifiers(IDENT_SRC, offset);
        // `a` and `b` are params of `add`, but we're outside that fn, so they won't
        // appear. `main` has no params. Check that the fn names appear.
        assert!(
            idents.iter().any(|i| i.name == "add"),
            "add function should appear in identifiers"
        );
    }

    /// id_pos3: module-level fns appear with MODULE rank.
    #[test]
    fn id_pos3_module_fns_appear() {
        let idents = collect_scope_identifiers(IDENT_SRC, IDENT_SRC.len());
        let add = idents.iter().find(|i| i.name == "add");
        assert!(add.is_some(), "add fn must be found");
        assert_eq!(add.unwrap().rank, RANK_MODULE);
    }

    /// id_pos4: prelude types appear (int, str, bool, Option, Result).
    #[test]
    fn id_pos4_prelude_types_appear() {
        let idents = collect_scope_identifiers(IDENT_SRC, IDENT_SRC.len());
        for name in &["int", "str", "bool", "Option", "Result"] {
            assert!(
                idents.iter().any(|i| &i.name == name),
                "{} should be in prelude",
                name
            );
        }
    }

    /// id_pos5: completion_for returns CompletionItems with labels matching idents.
    #[test]
    fn id_pos5_completion_for_fn_body() {
        let src = "module t\nfn f() -> () {\n    ro myvar int = 1\n    ";
        let offset = src.len();
        let items = completion_for(src, offset);
        assert!(
            has_label(&items, "myvar"),
            "myvar should appear in completion"
        );
        assert!(has_label(&items, "ro"), "ro keyword should appear");
        assert!(!has_label(&items, "let"), "let must NOT appear — removed in Plan 114");
    }

    /// id_neg1: empty source returns prelude + no panic.
    #[test]
    fn id_neg1_empty_source_no_panic() {
        let items = completion_for("", 0);
        // Prelude items + keywords at top level
        assert!(!items.is_empty(), "non-empty result expected for empty src");
    }

    /// id_neg2: cursor past end of source is handled gracefully.
    #[test]
    fn id_neg2_offset_past_end_no_panic() {
        let src = "fn f() => ()";
        let items = completion_for(src, src.len() + 1000);
        let _ = items; // No panic required.
    }

    // ─────────────────────────────────────────────────────────────────────────
    // 104.3.3 — Method-dot (Plan 104.10 Ф.5 will add type-driven coverage)
    // ─────────────────────────────────────────────────────────────────────────

    /// md_pos1: detect_context returns MethodDot for `x.`.
    #[test]
    fn md_pos1_context_detection_method_dot() {
        let src = "module t\nfn f() -> () {\n    ro x int = 5\n    x.";
        let ctx = detect_context(src, src.len());
        assert!(
            matches!(ctx, Some(CompletionContext::MethodDot { .. })),
            "expected MethodDot context, got {:?}",
            ctx
        );
    }

    /// md_pos2: user-defined receiver method appears in completions.
    #[test]
    fn md_pos2_user_defined_method() {
        let src = "module t\nfn Foo @greet() -> str => \"hello\"\nfn f() -> () {\n    ro x Foo = Foo {}\n    x.";
        let items = method_items(src, src.len());
        assert!(
            has_label(&items, "greet"),
            "user-defined method greet should appear"
        );
    }

    /// md_pos3: user-defined method appears when type is unknown (show all module methods).
    #[test]
    fn md_pos3_unknown_type_shows_module_methods() {
        let src = "module t\nfn Bar @run() -> () => ()\nfn f() -> () {\n    ro x = Bar {}\n    x.";
        let items = method_items(src, src.len());
        assert!(
            has_label(&items, "run"),
            "module method should appear even when type annotation absent"
        );
    }

    /// md_neg1: no dot → method_items returns empty.
    #[test]
    fn md_neg1_no_dot_returns_empty() {
        let src = "module t\nfn f() -> () {\n    ro x int = 5\n    x";
        let items = method_items(src, src.len());
        assert!(items.is_empty(), "no dot should return empty method completions");
    }

    /// md_neg2: cursor in comment → no completions.
    #[test]
    fn md_neg2_cursor_in_comment_no_completions() {
        let src = "module t\n// x.";
        let items = completion_for(src, src.len());
        assert!(items.is_empty(), "cursor in comment should yield no completions");
    }

    /// md_edge1: cursor in string → no completions.
    #[test]
    fn md_edge1_cursor_in_string_no_completions() {
        let src = "module t\nfn f() -> () {\n    ro s str = \"x.";
        let items = completion_for(src, src.len());
        assert!(items.is_empty(), "cursor in string should yield no completions");
    }

    /// md_edge2: number before dot (3.) is NOT a method call.
    #[test]
    fn md_edge2_number_dot_not_method_call() {
        let src = "module t\nfn f() -> () {\n    ro x = 3.";
        // If we put cursor right after "3.", context should be FnBody or MethodDot
        // but the detect_method_dot heuristic should NOT return numeric as obj_text.
        let ctx = detect_context(src, src.len());
        // The context might be MethodDot (3.) or FnBody — but either way, no crash.
        match ctx {
            Some(CompletionContext::MethodDot { obj_text }) => {
                // If it is MethodDot, obj_text must NOT be purely numeric (our guard).
                assert!(
                    !obj_text.chars().all(|c| c.is_ascii_digit()),
                    "numeric before dot should not produce method completion"
                );
            }
            _ => {} // FnBody or None — acceptable
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // 104.3.4 — Import path (4 pos + 1 neg)
    // ─────────────────────────────────────────────────────────────────────────

    /// imp_pos1: empty prefix returns top-level modules including `std`.
    #[test]
    fn imp_pos1_empty_prefix_returns_std() {
        let items = import_items(&[]);
        assert!(has_label(&items, "std"), "std should appear for empty prefix");
    }

    /// imp_pos2: `["std"]` prefix returns `std.*` submodules.
    #[test]
    fn imp_pos2_std_prefix_returns_submodules() {
        let prefix = vec!["std".to_string()];
        let items = import_items(&prefix);
        assert!(
            has_label(&items, "collections"),
            "collections should appear under std.*"
        );
        assert!(has_label(&items, "encoding"), "encoding under std");
        assert!(has_label(&items, "net"), "net under std");
        // Written since this test was -- the list follows the filesystem.
        assert!(has_label(&items, "io"), "io under std (std/src/io)");
        assert!(has_label(&items, "math"), "math under std (std/src/math)");
        // [M-104.10-hardcode-lists]: stale modules must NOT be advertised.
        assert!(!has_label(&items, "sync"), "std.sync does not exist");
    }

    /// imp_pos3: `["std", "collections"]` prefix returns vec, hashmap, set.
    #[test]
    fn imp_pos3_collections_returns_submodules() {
        let prefix = vec!["std".to_string(), "collections".to_string()];
        let items = import_items(&prefix);
        assert!(has_label(&items, "vec"), "vec under std.collections");
        assert!(has_label(&items, "hash_map"), "hash_map under std.collections");
        assert!(has_label(&items, "set"), "set under std.collections");
        // [M-104.10-hardcode-lists]: `map` never existed (it is `hash_map`).
        assert!(!has_label(&items, "map"), "std.collections.map does not exist");
        assert!(!has_label(&items, "hashmap"), "hashmap was renamed to hash_map");
    }

    /// imp_pos4: import_items returns MODULE kind items with sort_text.
    #[test]
    fn imp_pos4_item_structure() {
        let prefix = vec!["std".to_string()];
        let items = import_items(&prefix);
        for item in &items {
            assert_eq!(item.kind, Some(CompletionItemKind::MODULE));
            assert!(item.sort_text.is_some());
        }
    }

    /// imp_neg1: unknown prefix returns empty list.
    #[test]
    fn imp_neg1_unknown_prefix_empty() {
        let prefix = vec!["nonexistent_module_xyz".to_string()];
        let items = import_items(&prefix);
        assert!(items.is_empty(), "unknown prefix should return empty");
    }

    // ─────────────────────────────────────────────────────────────────────────
    // 104.3.5 — Ranking (3 pos)
    // ─────────────────────────────────────────────────────────────────────────

    /// rank_pos1: local binding sorts before module fn.
    #[test]
    fn rank_pos1_local_before_module() {
        let src = "module t\nfn myFn() -> () {}\nfn f() -> () {\n    ro myVar int = 1\n    m";
        let items = completion_for(src, src.len());
        let myvar_sort = items.iter()
            .find(|i| i.label == "myVar")
            .and_then(|i| i.sort_text.as_deref())
            .unwrap_or("");
        let myfn_sort = items.iter()
            .find(|i| i.label == "myFn")
            .and_then(|i| i.sort_text.as_deref())
            .unwrap_or("zzz");
        assert!(
            myvar_sort < myfn_sort,
            "local myVar ({}) should sort before module myFn ({})",
            myvar_sort,
            myfn_sort
        );
    }

    /// rank_pos2: module item sorts before prelude.
    #[test]
    fn rank_pos2_module_before_prelude() {
        let src = "module t\nfn myHelper() -> () {}\nfn f() -> () {\n    ";
        let items = completion_for(src, src.len());
        let module_sort = items.iter()
            .find(|i| i.label == "myHelper")
            .and_then(|i| i.sort_text.as_deref())
            .unwrap_or("zz");
        let prelude_sort = items.iter()
            .find(|i| i.label == "int")
            .and_then(|i| i.sort_text.as_deref())
            .unwrap_or("aa");
        assert!(
            module_sort < prelude_sort,
            "module myHelper ({}) should sort before prelude int ({})",
            module_sort,
            prelude_sort
        );
    }

    /// rank_pos3: keyword sorts after identifier.
    #[test]
    fn rank_pos3_ident_before_keyword() {
        let src = "module t\nfn f() -> () {\n    ro mylocal int = 1\n    ";
        let items = completion_for(src, src.len());
        let local_sort = items.iter()
            .find(|i| i.label == "mylocal")
            .and_then(|i| i.sort_text.as_deref())
            .unwrap_or("zz");
        // `ro` is the fn-body binding keyword (replaces removed `let`)
        let kw_sort = items.iter()
            .find(|i| i.label == "ro")
            .and_then(|i| i.sort_text.as_deref())
            .unwrap_or("aa");
        assert!(
            local_sort < kw_sort,
            "local mylocal ({}) should sort before keyword ro ({})",
            local_sort,
            kw_sort
        );
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Additional context detection tests
    // ─────────────────────────────────────────────────────────────────────────

    /// ctx_pos1: detect TopLevel at module level.
    #[test]
    fn ctx_pos1_top_level() {
        let src = "module t\n";
        let ctx = detect_context(src, src.len());
        assert_eq!(ctx, Some(CompletionContext::TopLevel));
    }

    /// ctx_pos2: detect FnBody inside braces.
    #[test]
    fn ctx_pos2_fn_body() {
        let src = "module t\nfn f() -> () {\n    ";
        let ctx = detect_context(src, src.len());
        assert_eq!(ctx, Some(CompletionContext::FnBody));
    }

    /// ctx_pos3: detect Import on import line.
    #[test]
    fn ctx_pos3_import_context() {
        let src = "module t\nimport std.collections.";
        let ctx = detect_context(src, src.len());
        assert!(
            matches!(ctx, Some(CompletionContext::Import { .. })),
            "expected Import context"
        );
    }

    /// is_in_comment test.
    #[test]
    fn ctx_comment_detection() {
        let src = "fn f() => () // comment";
        assert!(is_in_comment(src, src.len()), "should be in comment");
        assert!(!is_in_comment(src, 5), "should not be in comment before //");
    }

    /// is_in_string test.
    #[test]
    fn ctx_string_detection() {
        let src = r#"ro s str = "hello"#;
        assert!(is_in_string(src, src.len()), "cursor inside string literal");
        assert!(!is_in_string(src, 5), "cursor before string is not in string");
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Ф.13 — Lazy completion resolve (5 pos + 3 neg)
    // ─────────────────────────────────────────────────────────────────────────

    fn find_item<'a>(items: &'a [CompletionItem], label: &str) -> &'a CompletionItem {
        items
            .iter()
            .find(|i| i.label == label)
            .unwrap_or_else(|| panic!("no completion item labelled {label:?}"))
    }

    /// resolve_pos1: a keyword item ships with NO detail/documentation in the
    /// initial list; resolve fills both.
    #[test]
    fn resolve_pos1_keyword_lazy_doc() {
        let items = completion_for("module t\n", "module t\n".len());
        let fn_kw = find_item(&items, "fn");
        assert_eq!(fn_kw.kind, Some(CompletionItemKind::KEYWORD));
        assert!(fn_kw.detail.is_none(), "initial keyword must have no detail");
        assert!(
            fn_kw.documentation.is_none(),
            "initial keyword must have no documentation (deferred)"
        );
        assert!(fn_kw.data.is_some(), "keyword must carry a resolve descriptor");

        let resolved = resolve_completion_item(fn_kw.clone());
        assert!(resolved.detail.is_some(), "resolve must add detail");
        match resolved.documentation {
            Some(Documentation::MarkupContent(m)) => {
                assert!(m.value.contains("keyword"), "doc should mention keyword: {}", m.value)
            }
            other => panic!("resolve must add markdown documentation, got {other:?}"),
        }
    }

    /// resolve_pos2: a prelude identifier ships without its verbose one-line doc;
    /// resolve re-derives it from the prelude table.
    #[test]
    fn resolve_pos2_prelude_lazy_doc() {
        let src = "module t\nfn f() -> () {\n    ";
        let items = completion_for(src, src.len());
        let int_item = find_item(&items, "int");
        assert!(int_item.detail.is_none(), "initial prelude item must have no detail");
        assert!(int_item.documentation.is_none(), "initial prelude item must have no doc");

        let resolved = resolve_completion_item(int_item.clone());
        assert_eq!(
            resolved.detail.as_deref(),
            Some("64-bit signed integer (Nova's universal integer type)")
        );
        assert!(resolved.documentation.is_some(), "resolve must add prelude doc");
    }

    /// resolve_pos3: a snippet ships with its `insert_text` (needed to apply) but
    /// no detail/documentation; resolve fills the descriptive fields.
    #[test]
    fn resolve_pos3_snippet_lazy_doc() {
        let src = "module t\nfn f() -> () {\n    ";
        let items = completion_for(src, src.len());
        // `if-let` is a snippet-only label (not also a keyword), so dedup keeps it.
        let snip = find_item(&items, "if-let");
        assert_eq!(snip.kind, Some(CompletionItemKind::SNIPPET));
        assert!(snip.insert_text.is_some(), "snippet must keep insert_text inline");
        assert!(snip.detail.is_none(), "initial snippet must have no detail");
        assert!(snip.documentation.is_none(), "initial snippet must have no doc");

        let resolved = resolve_completion_item(snip.clone());
        assert!(resolved.detail.is_some(), "resolve must add snippet detail");
        assert!(resolved.documentation.is_some(), "resolve must add snippet doc");
        // insert_text must survive resolve untouched.
        assert_eq!(resolved.insert_text, snip.insert_text);
    }

    /// The invariant the repair exists for, and had no test until 2026-08-18:
    /// whatever it produces must PARSE under the grammar as it stands today.
    /// Nothing here is currently broken -- this test exists because the failure
    /// mode is SILENT. An unparseable repaired buffer yields no `expr_types`, so
    /// method completion drops to the textual scan and quietly loses stdlib
    /// methods, cross-file methods and doc comments, with no error anywhere.
    /// A grammar change is exactly what would trigger it, which is why the
    /// shapes below are written the way a user actually types.
    #[test]
    fn repair_pos1_repaired_buffer_parses_for_every_interactive_shape() {
        let shapes = [
            ("method at end of fn",
             "module t\ntype Foo { a int }\nfn Foo @m() -> str => \"x\"\nfn f() -> () {\n    ro x Foo = Foo.new(1)\n    x."),
            ("inside a call",
             "module t\nfn f() -> () {\n    ro s = \"a\"\n    println(s."),
            // The index must be a BINDING, not a literal: `v[0.` is a float
            // literal being typed, and `method_dot_offset` is right to say so.
            ("inside an index",
             "module t\nfn f() -> () {\n    ro v = []int.of(1)\n    ro i = 0\n    ro y = v[i]."),
            ("nested blocks",
             "module t\nfn f() -> () {\n    if true {\n        ro s = \"a\"\n        s."),
            ("after a string with a brace in it",
             "module t\nfn f() -> () {\n    ro s = \"{ not a block\"\n    s."),
            ("chained receiver",
             "module t\nfn f() -> () {\n    ro s = \"a\"\n    ro t = s.trim()."),
        ];
        for (name, src) in shapes {
            let dot = method_dot_offset(src, src.len())
                .unwrap_or_else(|| panic!("{name}: no dot found"));
            let repaired = repair_completion_buffer(src, dot);
            if let Err(e) = crate::compiler::parse_guarded(&repaired) {
                panic!("{name}: repaired buffer does not parse: {}\n{:?}", e.message, repaired);
            }
            // The whole point of repairing rather than truncating: every offset
            // up to the cursor must still mean what it meant.
            assert_eq!(&repaired[..dot], &src[..dot], "{name}: prefix moved");
        }
    }

    /// resolve_pos4: method documentation is deferred — the initial method item
    /// has no `documentation`; resolve attaches the stashed doc comment.
    #[test]
    fn resolve_pos4_method_lazy_doc() {
        // Two things here are LOAD-BEARING, and the test spent a while red
        // because both were missing. `type Foo` must be declared, or the
        // receiver type cannot be inferred and completion falls back to the
        // textual scan, which carries no doc at all. And the value must be built
        // with `Foo.new(..)`: the `Foo {}` record-literal form is retired, and a
        // buffer containing it does not parse, which empties `expr_types` for
        // the same reason. Either way the test would have passed or failed for
        // reasons unrelated to lazy doc resolution.
        let src = "module t\ntype Foo { a int }\nfn Foo @greet() -> str => \"hi\"\n/// Greets loudly.\nfn Foo @shout() -> str => \"HI\"\nfn f() -> () {\n    ro x Foo = Foo.new(1)\n    x.";
        let items = method_items(src, src.len());
        let shout = find_item(&items, "shout");
        assert!(
            shout.documentation.is_none(),
            "initial method item must not ship documentation"
        );
        assert!(shout.detail.is_some(), "method signature detail stays inline");

        let resolved = resolve_completion_item(shout.clone());
        match resolved.documentation {
            Some(Documentation::MarkupContent(m)) => {
                assert!(m.value.contains("Greets loudly"), "doc mismatch: {}", m.value)
            }
            other => panic!("resolve must attach the method doc, got {other:?}"),
        }
    }

    /// resolve_pos5: the initial list is genuinely lighter — NO keyword / snippet
    /// / prelude item carries documentation before resolve.
    #[test]
    fn resolve_pos5_initial_list_has_no_heavy_docs() {
        let src = "module t\nfn f() -> () {\n    ";
        let items = completion_for(src, src.len());
        for item in &items {
            let is_static_family = matches!(
                item.kind,
                Some(CompletionItemKind::KEYWORD) | Some(CompletionItemKind::SNIPPET)
            );
            if is_static_family {
                assert!(
                    item.documentation.is_none(),
                    "initial list must not ship documentation for {:?}",
                    item.label
                );
            }
        }
    }

    /// resolve_neg1: an item without `data` (e.g. a local variable) resolves to
    /// itself, unchanged and without panic.
    #[test]
    fn resolve_neg1_no_data_graceful() {
        let item = CompletionItem {
            label: "myLocal".to_string(),
            kind: Some(CompletionItemKind::VARIABLE),
            detail: Some("int".to_string()),
            ..Default::default()
        };
        let resolved = resolve_completion_item(item.clone());
        assert_eq!(resolved.label, "myLocal");
        assert!(resolved.documentation.is_none(), "no data → no doc added");
        assert_eq!(resolved.detail, item.detail, "existing detail preserved");
    }

    /// resolve_neg2: a malformed / unknown `data` family is ignored gracefully.
    #[test]
    fn resolve_neg2_unknown_family_graceful() {
        let item = CompletionItem {
            label: "weird".to_string(),
            data: Some(serde_json::json!({ "f": "totally-unknown", "k": "x" })),
            ..Default::default()
        };
        let resolved = resolve_completion_item(item);
        assert!(resolved.documentation.is_none(), "unknown family adds nothing");
        assert!(resolved.detail.is_none());
    }

    /// resolve_neg3: a `data` descriptor whose key does not exist in its table
    /// resolves gracefully (no doc, no panic).
    #[test]
    fn resolve_neg3_unknown_key_graceful() {
        let item = CompletionItem {
            label: "notakeyword".to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            data: Some(serde_json::json!({ "f": "kw", "k": "notakeyword", "c": "top" })),
            ..Default::default()
        };
        let resolved = resolve_completion_item(item);
        assert!(resolved.documentation.is_none(), "unknown keyword key adds no doc");
        assert!(resolved.detail.is_none());
    }
}
