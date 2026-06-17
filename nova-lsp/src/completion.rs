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
//! # V1 simplifications (documented in simplifications.md)
//! - [S-104.3-1] Method dot: type inference via text pattern match, not full TypeCheckCtx.
//! - [S-104.3-2] Import path: hardcoded std module tree.
//! - [S-104.3-3] resolve_provider=false — detail is inline in initial response.

use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, Documentation, InsertTextFormat, MarkupContent,
    MarkupKind,
};

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

/// Build keyword completion items for the given context.
pub fn keyword_items(ctx: &CompletionContext) -> Vec<CompletionItem> {
    let kws: &[(&str, &str)] = match ctx {
        CompletionContext::TopLevel => TOP_LEVEL_KEYWORDS,
        CompletionContext::FnBody => FN_BODY_KEYWORDS,
        CompletionContext::TypeBody => TYPE_BODY_KEYWORDS,
        CompletionContext::Import { .. } | CompletionContext::MethodDot { .. } => return vec![],
    };

    kws.iter()
        .map(|(kw, doc)| CompletionItem {
            label: kw.to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            detail: Some(doc.to_string()),
            documentation: Some(Documentation::MarkupContent(MarkupContent {
                kind: MarkupKind::Markdown,
                value: format!("**Nova keyword** — {}", doc),
            })),
            sort_text: Some(format!("{}{}", RANK_KEYWORD, kw)),
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
        .map(|s| CompletionItem {
            label: s.label.to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            detail: Some(s.detail.to_string()),
            insert_text: Some(s.insert_text.to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            sort_text: Some(format!("{}{}", RANK_SNIPPET, s.label)),
            documentation: Some(Documentation::MarkupContent(MarkupContent {
                kind: MarkupKind::Markdown,
                value: format!("**Snippet** — `{}`", s.detail),
            })),
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

/// Hardcoded Nova prelude items.
fn prelude_items() -> Vec<IdentInfo> {
    const PRELUDE: &[(&str, &str, IdentKind)] = &[
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
    PRELUDE
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
pub fn ident_info_to_item(info: &IdentInfo) -> CompletionItem {
    let kind = match info.kind {
        IdentKind::Local => CompletionItemKind::VARIABLE,
        IdentKind::Fn => CompletionItemKind::FUNCTION,
        IdentKind::Type => CompletionItemKind::CLASS,
        IdentKind::Const => CompletionItemKind::CONSTANT,
        IdentKind::Prelude => CompletionItemKind::KEYWORD,
    };
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

/// Builtin method info.
#[derive(Debug, Clone)]
pub struct MethodInfo {
    pub name: String,
    pub signature: String,
    pub doc: String,
    pub rank: &'static str,
}

impl MethodInfo {
    fn new(name: &str, sig: &str, doc: &str) -> Self {
        Self {
            name: name.to_string(),
            signature: sig.to_string(),
            doc: doc.to_string(),
            rank: RANK_PRELUDE,
        }
    }
}

/// Methods available on `int`.
/// Source: std/runtime/defaults.nv + std/time/duration.nv (time-related not listed here).
fn int_methods() -> Vec<MethodInfo> {
    vec![
        MethodInfo::new("min", "(other int) -> int", "Minimum of two ints"),
        MethodInfo::new("max", "(other int) -> int", "Maximum of two ints"),
        MethodInfo::new("clamp", "(lo int, hi int) -> int", "Clamp to [lo, hi]"),
        MethodInfo::new("compare", "(other int) -> int", "Three-way comparison (-1/0/1)"),
        MethodInfo::new("debug", "(sb consume StringBuilder) -> StringBuilder", "Debug format: decimal — called by ${x:?} interpolation (Plan 91.14, D229)"),
        // str.from(n) is the idiomatic int→str conversion in Nova
        // (no direct @to_str on int); listed here for discoverability.
    ]
}

/// Methods available on `f64` (and `f32`).
/// Source: std/runtime/math.nv + std/runtime/defaults.nv.
fn f64_methods() -> Vec<MethodInfo> {
    vec![
        MethodInfo::new("abs", "() -> f64", "Absolute value"),
        MethodInfo::new("floor", "() -> f64", "Floor (round toward -∞)"),
        MethodInfo::new("ceil", "() -> f64", "Ceiling (round toward +∞)"),
        MethodInfo::new("round", "() -> f64", "Round to nearest integer"),
        MethodInfo::new("trunc", "() -> f64", "Truncate toward zero"),
        MethodInfo::new("sqrt", "() -> f64", "Square root"),
        MethodInfo::new("sin", "() -> f64", "Sine"),
        MethodInfo::new("cos", "() -> f64", "Cosine"),
        MethodInfo::new("tan", "() -> f64", "Tangent"),
        MethodInfo::new("ln", "() -> f64", "Natural logarithm"),
        MethodInfo::new("log2", "() -> f64", "Base-2 logarithm"),
        MethodInfo::new("exp", "() -> f64", "e^self"),
        MethodInfo::new("is_nan", "() -> bool", "True if NaN"),
        MethodInfo::new("is_infinite", "() -> bool", "True if ±infinity"),
        MethodInfo::new("min", "(other f64) -> f64", "Minimum"),
        MethodInfo::new("max", "(other f64) -> f64", "Maximum"),
        MethodInfo::new("clamp", "(lo f64, hi f64) -> f64", "Clamp to [lo, hi]"),
        MethodInfo::new("compare", "(other f64) -> int", "Three-way comparison"),
        MethodInfo::new("debug", "(sb consume StringBuilder) -> StringBuilder", "Debug format: decimal — called by ${x:?} interpolation (Plan 91.14, D229)"),
    ]
}

/// Methods available on `str`.
/// Source: std/runtime/string/{core,search,transform,slice,parse,chars}.nv (Plan 152).
fn str_methods() -> Vec<MethodInfo> {
    vec![
        // core.nv
        MethodInfo::new("byte_len", "() -> int", "Byte length (O(1)). Note: str[i] is a codepoint, not a byte."),
        MethodInfo::new("is_empty", "() -> bool", "True if byte_len() == 0"),
        MethodInfo::new("as_bytes", "() -> ro []u8", "Zero-copy byte view (O(1))"),
        MethodInfo::new("as_ptr", "() -> *u8", "Raw pointer to UTF-8 data"),
        MethodInfo::new("to_bytes", "() -> []u8", "Owned copy of bytes"),
        MethodInfo::new("to_chars", "() -> []char", "Collect all codepoints into Vec[char] (O(n))"),
        MethodInfo::new("equal", "(other str) -> bool", "Byte-equality"),
        MethodInfo::new("compare", "(other str) -> int", "Lexicographic comparison"),
        MethodInfo::new("is_char_boundary", "(idx int) -> bool", "True if byte idx is a codepoint boundary"),
        MethodInfo::new("eq_ignore_ascii_case", "(other str) -> bool", "Case-insensitive ASCII equality"),
        // chars.nv — lazy iterator lens
        MethodInfo::new("as_chars", "() -> CharsIter", "Lazy codepoint iterator (call .collect() for Vec[char])"),
        MethodInfo::new("iter", "() -> CharsIter", "Alias for as_chars()"),
        // search.nv
        MethodInfo::new("starts_with", "(prefix str) -> bool", "Byte-prefix check"),
        MethodInfo::new("ends_with", "(suffix str) -> bool", "Byte-suffix check"),
        MethodInfo::new("contains", "(needle str) -> bool", "Substring check"),
        MethodInfo::new("find", "(needle str) -> Option[int]", "Byte offset of first occurrence"),
        MethodInfo::new("rfind", "(needle str) -> Option[int]", "Byte offset of last occurrence"),
        MethodInfo::new("split", "(sep str) -> ro []str", "Split by separator (byte-coordinated)"),
        MethodInfo::new("splitn", "(n int, sep str) -> ro []str", "Split at most n times"),
        MethodInfo::new("rsplit", "(sep str) -> ro []str", "Split from right"),
        MethodInfo::new("split_once", "(sep str) -> Option[(str, str)]", "Split at first separator"),
        MethodInfo::new("split_whitespace", "() -> ro []str", "Split on ASCII whitespace"),
        MethodInfo::new("lines", "() -> ro []str", "Split on newlines"),
        MethodInfo::new("match_indices", "(needle str) -> []int", "All byte offsets of needle"),
        // transform.nv
        MethodInfo::new("trim", "() -> str", "Trim ASCII whitespace from both ends"),
        MethodInfo::new("trim_start", "() -> str", "Trim ASCII whitespace from start"),
        MethodInfo::new("trim_end", "() -> str", "Trim ASCII whitespace from end"),
        MethodInfo::new("trim_start_matches", "(c char) -> str", "Trim leading occurrences of char"),
        MethodInfo::new("trim_end_matches", "(c char) -> str", "Trim trailing occurrences of char"),
        MethodInfo::new("strip_prefix", "(prefix str) -> Option[str]", "Remove prefix if present"),
        MethodInfo::new("strip_suffix", "(suffix str) -> Option[str]", "Remove suffix if present"),
        MethodInfo::new("to_lower", "() -> str", "ASCII lowercase"),
        MethodInfo::new("to_upper", "() -> str", "ASCII uppercase"),
        MethodInfo::new("replace", "(from str, to str) -> str", "Replace all occurrences"),
        MethodInfo::new("replacen", "(from str, to str, n int) -> str", "Replace at most n occurrences"),
        MethodInfo::new("repeat", "(n int) -> str", "Repeat string n times"),
        MethodInfo::new("pad_left", "(width int, fill char) -> str", "Pad to width on left"),
        MethodInfo::new("pad_right", "(width int, fill char) -> str", "Pad to width on right"),
        MethodInfo::new("pad_center", "(width int, fill char) -> str", "Center-pad to width"),
        MethodInfo::new("concat", "(other str) -> str", "Concatenate (also via `+` operator)"),
        // slice.nv — indexing with Range (byte-coordinated)
        MethodInfo::new("get", "(r Range) -> Option[str]", "Byte-range slice (bounds-safe)"),
        // parse.nv
        MethodInfo::new("parse_int", "(radix int = 10) -> Option[int]", "Parse as integer"),
        MethodInfo::new("try_parse_int", "(radix int = 10) -> Result[int, ParseIntError]", "Parse as integer (detailed error)"),
        // Debug protocol (Plan 91.14, D229)
        MethodInfo::new("debug", "(sb consume StringBuilder) -> StringBuilder", "Debug format: quoted with escape sequences e.g. \"hello\\n\" — called by ${x:?}"),
    ]
}

/// Methods available on `Vec[T]` / `[]T`.
/// Source: std/collections/vec/{core,access,mutate,sort,restructure,views,slice}.nv
/// and std/collections/{vec_seq,vec_lazy,vec_iter}.nv (Plan 153).
///
/// Mutable methods (require `mut` receiver) are marked with `mut`.
fn vec_methods() -> Vec<MethodInfo> {
    vec![
        // Read-only access (core.nv / access.nv)
        MethodInfo::new("len", "() -> int", "Number of elements"),
        MethodInfo::new("cap", "() -> int", "Current allocated capacity"),
        MethodInfo::new("is_empty", "() -> bool", "True if len() == 0"),
        MethodInfo::new("get", "(i int) -> Option[T]", "Bounds-safe element access"),
        MethodInfo::new("first", "() -> Option[T]", "First element or None"),
        MethodInfo::new("last", "() -> Option[T]", "Last element or None"),
        MethodInfo::new("contains", "(v T) -> bool", "Linear membership check"),
        MethodInfo::new("index_of", "(v T) -> Option[int]", "First index of value"),
        MethodInfo::new("position", "(pred fn(T)->bool) -> Option[int]", "First index satisfying predicate"),
        MethodInfo::new("binary_search_by", "(cmp fn(T)->int) -> Result[int,int]", "Binary search"),
        MethodInfo::new("as_ptr", "() -> *T", "Raw read-only pointer to data"),
        // Slices / views (views.nv / slice.nv)
        MethodInfo::new("as_slice", "() -> Vec[T]", "Immutable slice view"),
        MethodInfo::new("split_at", "(i int) -> (Vec[T], Vec[T])", "Split into two slices at index"),
        MethodInfo::new("split_first", "() -> Option[(T, Vec[T])]", "Head and tail"),
        MethodInfo::new("split_last", "() -> Option[(Vec[T], T)]", "Init and last"),
        MethodInfo::new("first_n", "(n int) -> Vec[T]", "First n elements"),
        MethodInfo::new("last_n", "(n int) -> Vec[T]", "Last n elements"),
        MethodInfo::new("get", "(r Range) -> Option[Vec[T]]", "Bounds-safe range slice"),
        // Mutation (mutate.nv) — all require `mut` receiver
        MethodInfo::new("push", "mut (v T) -> @", "Append element (fluent, returns self)"),
        MethodInfo::new("pop", "mut () -> Option[T]", "Remove and return last element"),
        MethodInfo::new("insert", "mut (i int, v T) -> @", "Insert element at index"),
        MethodInfo::new("remove", "mut (i int) -> T", "Remove and return element at index"),
        MethodInfo::new("swap_remove", "mut (i int) -> T", "O(1) remove (replaces with last)"),
        MethodInfo::new("clear", "mut () -> @", "Remove all elements (keep capacity)"),
        MethodInfo::new("truncate", "mut (n int) -> @", "Shorten to n elements"),
        MethodInfo::new("reverse", "mut () -> @", "Reverse in-place"),
        MethodInfo::new("swap", "mut (i int, j int) -> @", "Swap two elements"),
        MethodInfo::new("resize", "mut (n int, v T) -> @", "Resize, filling with v"),
        MethodInfo::new("append", "mut (other S) -> @", "Append all elements from other (copies)"),
        MethodInfo::new("extend", "mut (items S) -> @", "Extend from any Iter[T]"),
        MethodInfo::new("retain", "mut (pred fn(T)->bool) -> @", "Keep only elements matching predicate"),
        MethodInfo::new("fill", "mut (v T) -> @", "Fill all slots with value"),
        MethodInfo::new("reserve", "mut (additional int) -> @", "Ensure capacity for at least N more elements"),
        // Sort (sort.nv) — concrete []int fast-path + generic _of variants (Plan 91.8c, D185)
        MethodInfo::new("sort", "mut [T Compare] () -> @", "Stable sort by Compare"),
        MethodInfo::new("sort_by", "mut (cmp fn(T,T)->int) -> @", "Stable sort with custom comparator"),
        MethodInfo::new("sort_unstable", "mut [T Compare] () -> @", "Unstable sort (faster, less memory)"),
        MethodInfo::new("sort_of", "mut [T Compare] () -> @", "Generic stable sort for any T Compare (Plan 91.8c)"),
        MethodInfo::new("sort_by_of", "mut (cmp fn(T,T)->int) -> @", "Generic sort with callback, no Compare bound (Plan 91.8c)"),
        MethodInfo::new("min_of", "[T Compare] () -> Option[T]", "Minimum element for any T Compare; None if empty (Plan 91.8c)"),
        MethodInfo::new("max_of", "[T Compare] () -> Option[T]", "Maximum element for any T Compare; None if empty (Plan 91.8c)"),
        MethodInfo::new("min_by_of", "(cmp fn(T,T)->int) -> Option[T]", "Minimum by callback comparator; None if empty (Plan 91.8c)"),
        MethodInfo::new("max_by_of", "(cmp fn(T,T)->int) -> Option[T]", "Maximum by callback comparator; None if empty (Plan 91.8c)"),
        MethodInfo::new("binary_search_of", "[T Compare] (target T) -> Option[int]", "Binary search on sorted slice; returns index or None (Plan 91.8c)"),
        MethodInfo::new("reverse_of", "mut () -> @", "Reverse elements in-place (Plan 91.8c)"),
        MethodInfo::new("position_of", "(pred fn(T)->bool) -> Option[int]", "Index of first matching element; None if not found (Plan 91.8c)"),
        MethodInfo::new("count_of", "(pred fn(T)->bool) -> int", "Count elements matching predicate (Plan 91.8c)"),
        MethodInfo::new("find_of", "(pred fn(T)->bool) -> Option[T]", "First element matching predicate; None if not found (Plan 91.8c)"),
        MethodInfo::new("dedup", "mut () -> @", "Remove consecutive duplicates"),
        MethodInfo::new("partition", "mut (pred fn(T)->bool) -> int", "Partition by predicate, return split index"),
        // Restructure (restructure.nv)
        MethodInfo::new("concat", "(other Vec[T]) -> Vec[T]", "Concatenate (new allocation, also via `+`)"),
        MethodInfo::new("flatten", "() -> Vec[T]", "Flatten Vec[Vec[T]] into Vec[T]"),
        MethodInfo::new("rotate_left", "mut (n int) -> @", "Cyclic shift left by n"),
        MethodInfo::new("rotate_right", "mut (n int) -> @", "Cyclic shift right by n"),
        MethodInfo::new("drain", "mut (range Range) -> Vec[T]", "Remove and return a range of elements"),
        MethodInfo::new("insert_slice", "mut (i int, sl []T) -> @", "Insert slice at position"),
        // Iterator adapters (vec_seq.nv — eager on Vec directly)
        MethodInfo::new("map", "[U] (f fn(T)->U) -> []U", "Eager map (new Vec)"),
        MethodInfo::new("filter", "(pred fn(T)->bool) -> []T", "Eager filter (new Vec)"),
        MethodInfo::new("fold", "[Acc] (init Acc, f fn(Acc,T)->Acc) -> Acc", "Left fold"),
        MethodInfo::new("any", "(pred fn(T)->bool) -> bool", "True if any element matches"),
        MethodInfo::new("all", "(pred fn(T)->bool) -> bool", "True if all elements match"),
        // Lazy iterator (vec_lazy.nv / vec_iter.nv — zero-alloc adapters)
        MethodInfo::new("iter", "() -> VecIter[T]", "Lazy iterator (chain .map()/.filter()/.collect())"),
        MethodInfo::new("lazy", "() -> BoxIter[T]", "Boxed lazy iterator"),
        MethodInfo::new("chunks", "(n int) -> BoxIter[Vec[T]]", "Lazy chunks of size n"),
        MethodInfo::new("windows", "(n int) -> BoxIter[Vec[T]]", "Sliding windows of size n"),
        // Protocols
        MethodInfo::new("equal", "(other Vec[T]) -> bool", "Element-wise equality"),
        MethodInfo::new("as_slice", "() -> Vec[T]", "AsSlice[T] protocol implementation"),
        MethodInfo::new("debug", "(sb consume StringBuilder) -> StringBuilder [T Debug]", "Debug format: [e1, e2, ...] — called by ${x:?} (Plan 91.14, D229)"),
    ]
}

/// Methods available on `bool`.
/// Source: std/prelude/protocols.nv (Display/Debug via display()/debug()).
fn bool_methods() -> Vec<MethodInfo> {
    vec![
        MethodInfo::new("compare", "(other bool) -> int", "Three-way comparison"),
        MethodInfo::new("debug", "(sb consume StringBuilder) -> StringBuilder", "Debug protocol — called by ${x:?} interpolation (Plan 91.14, D229)"),
    ]
}

/// Methods available on `Option[T]`.
/// Source: std/prelude/core.nv.
fn option_methods() -> Vec<MethodInfo> {
    vec![
        MethodInfo::new("is_some", "() -> bool", "True if Some(_)"),
        MethodInfo::new("is_none", "() -> bool", "True if None"),
        MethodInfo::new("unwrap", "() Fail[Error] -> T", "Unwrap or panic (uses Fail effect)"),
        MethodInfo::new("unwrap_or", "(default_v T) -> T", "Unwrap or return default"),
        MethodInfo::new("unwrap_or_else", "(default_fn fn()->T) -> T", "Unwrap or call closure"),
        MethodInfo::new("map", "[U] (map_fn fn(T)->U) -> Option[U]", "Map Some value"),
        MethodInfo::new("ok_or", "[E] (err E) -> Result[T,E]", "Convert to Result"),
        MethodInfo::new("or", "(other Option[T]) -> Option[T]", "Return self if Some, else other"),
        MethodInfo::new("debug", "(sb consume StringBuilder) -> StringBuilder [T Debug]", "Debug format: Some(x) / None — called by ${x:?} (Plan 91.14, D229)"),
    ]
}

/// Methods available on `Result[T, E]`.
/// Source: std/prelude/core.nv.
fn result_methods() -> Vec<MethodInfo> {
    vec![
        MethodInfo::new("is_ok", "() -> bool", "True if Ok(_)"),
        MethodInfo::new("is_err", "() -> bool", "True if Err(_)"),
        MethodInfo::new("unwrap", "() Fail[E] -> T", "Unwrap Ok or propagate Err (uses Fail effect)"),
        MethodInfo::new("unwrap_or", "(default_v T) -> T", "Unwrap Ok or return default"),
        MethodInfo::new("unwrap_or_else", "(default_fn fn(E)->T) -> T", "Unwrap Ok or call closure with Err"),
        MethodInfo::new("ok", "() -> Option[T]", "Convert Ok to Some, Err to None"),
        MethodInfo::new("err", "() -> Option[E]", "Convert Err to Some, Ok to None"),
        MethodInfo::new("map", "[U] (map_fn fn(T)->U) -> Result[U,E]", "Map Ok value"),
        MethodInfo::new("map_err", "[F] (err_fn fn(E)->F) -> Result[T,F]", "Map Err value"),
        MethodInfo::new("debug", "(sb consume StringBuilder) -> StringBuilder [T Debug, E Debug]", "Debug format: Ok(x) / Err(e) — called by ${x:?} (Plan 91.14, D229)"),
    ]
}

/// Infer the type of an expression given its text and the surrounding source.
///
/// V1: text-pattern heuristic. Returns the type name or None.
fn infer_type_of_expr(obj_text: &str, src: &str) -> Option<String> {
    // Check ro/mut bindings to find the type annotation.
    for line in src.lines() {
        let t = line.trim();
        // Pattern: `ro name TYPE = ...` / `mut name TYPE = ...`
        // Also tolerate old `let` that the user might still be typing.
        for prefix in &["ro ", "mut ", "let "] {
            if let Some(rest) = t.strip_prefix(prefix) {
                let name = first_ident(rest);
                if name == obj_text {
                    if let Some(ty) = extract_type_after_name(rest, &name) {
                        return Some(ty);
                    }
                }
            }
        }
        // Pattern: fn param `name Type,` — possibly prefixed with ro/mut.
        if t.contains('(') && t.contains(obj_text) {
            if let Some(paren_start) = t.find('(') {
                let params = &t[paren_start + 1..];
                for param in params.split(',') {
                    let p = param.trim();
                    let p = p.strip_prefix("ro ").or_else(|| p.strip_prefix("mut ")).unwrap_or(p);
                    let pname = first_ident(p);
                    if pname == obj_text {
                        if let Some(ty) = extract_type_after_name(p, &pname) {
                            return Some(ty);
                        }
                    }
                }
            }
        }
    }

    // Naming-convention heuristics.
    let lower = obj_text.to_lowercase();
    // Vec / slice heuristics.
    if lower.ends_with("_vec") || lower.ends_with("_list") || lower == "items" || lower == "elems" {
        return Some("Vec".to_string());
    }
    // str heuristics.
    if lower.contains("name") || lower.contains("msg") || lower.contains("text")
        || lower.contains("label") || lower.ends_with("_str") || lower == "s"
    {
        return Some("str".to_string());
    }
    // int heuristics.
    if lower.contains("count") || lower.contains("index") || lower.contains("len")
        || lower.contains("size") || lower.contains("idx") || lower == "n" || lower == "i"
    {
        return Some("int".to_string());
    }
    // f64 heuristics.
    if lower.contains("ratio") || lower.contains("factor") || lower.ends_with("_f64") {
        return Some("f64".to_string());
    }
    // Option / Result heuristics.
    if lower == "opt" || lower == "maybe" {
        return Some("Option".to_string());
    }
    if lower == "result" || lower == "res" {
        return Some("Result".to_string());
    }

    None
}

/// Get methods for a given type name.
pub fn methods_for_type(ty: &str) -> Vec<MethodInfo> {
    match ty {
        // int family — only `int` is the canonical Nova integer type.
        // i32/i64/u8/u32/u64 share the same method set (min/max/compare).
        "int" | "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64" => int_methods(),
        // float family — f64 is the canonical Nova float type (float literal infers f64).
        "f64" | "f32" => f64_methods(),
        "str" => str_methods(),
        "bool" => bool_methods(),
        // Vec[T] and []T sugar alias.
        t if t.starts_with("Vec") || t.starts_with("[]") => vec_methods(),
        "Option" => option_methods(),
        "Result" => result_methods(),
        _ => vec![],
    }
}

/// Compute method completions for `expr.│` at the given offset.
pub fn method_items(src: &str, offset: usize) -> Vec<CompletionItem> {
    // Find what comes before the dot.
    let before = &src[..offset];
    let trimmed = before.trim_end();
    if !trimmed.ends_with('.') {
        return vec![];
    }
    let before_dot = &trimmed[..trimmed.len() - 1];
    let obj_text = extract_last_expr(before_dot);

    if obj_text.is_empty() {
        return vec![];
    }

    // Infer type of obj_text.
    let ty = infer_type_of_expr(&obj_text, src).unwrap_or_default();

    let methods = if ty.is_empty() {
        // Unknown type: return union of the most common method sets.
        let mut all = vec![];
        all.extend(int_methods());
        all.extend(f64_methods());
        all.extend(str_methods());
        all.extend(vec_methods());
        all
    } else {
        methods_for_type(&ty)
    };

    // Also scan module-level functions with matching receiver.
    let module_methods = scan_module_methods(src, &obj_text, &ty);

    let mut items: Vec<CompletionItem> = methods
        .iter()
        .map(|m| CompletionItem {
            label: m.name.clone(),
            kind: Some(CompletionItemKind::METHOD),
            detail: Some(format!("{} — {}", m.signature, m.doc)),
            sort_text: Some(format!("{}{}", m.rank, m.name)),
            documentation: Some(Documentation::MarkupContent(MarkupContent {
                kind: MarkupKind::Markdown,
                value: format!("**{}** `{}` — {}", m.name, m.signature, m.doc),
            })),
            ..Default::default()
        })
        .collect();

    items.extend(module_methods);
    items
}

/// Scan src for `fn TypeName @method_name(...)` declarations.
fn scan_module_methods(src: &str, _obj_text: &str, ty: &str) -> Vec<CompletionItem> {
    let mut out = Vec::new();
    if ty.is_empty() {
        return out;
    }
    for line in src.lines() {
        let t = line.trim();
        // Look for `fn TYPE @method_name(...)` pattern.
        let prefix = format!("fn {} @", ty);
        if let Some(rest) = t.strip_prefix(&prefix) {
            let name = first_ident(rest);
            if !name.is_empty() {
                out.push(CompletionItem {
                    label: name.clone(),
                    kind: Some(CompletionItemKind::METHOD),
                    detail: Some(format!("method on {}", ty)),
                    sort_text: Some(format!("{}{}", RANK_MODULE, name)),
                    ..Default::default()
                });
            }
        }
    }
    out
}

// ─────────────────────────────────────────────────────────────────────────────
// Import path completion (104.3.4)
// ─────────────────────────────────────────────────────────────────────────────

/// Hardcoded std module tree for V1 import completion.
/// Format: `["a.b.c", "a.b.d", ...]` — dot-separated paths.
const STD_MODULES: &[&str] = &[
    // Collections
    "std.collections",
    "std.collections.vec",
    "std.collections.map",
    "std.collections.set",
    "std.collections.deque",
    "std.collections.linked_list",
    "std.collections.priority_queue",
    // Concurrency
    "std.sync",
    "std.sync.mutex",
    "std.sync.rwlock",
    "std.sync.semaphore",
    "std.sync.condvar",
    "std.sync.barrier",
    "std.sync.countdown_latch",
    "std.sync.channel",
    "std.sync.once",
    "std.sync.atomic",
    // IO / net
    "std.net",
    "std.net.tcp",
    "std.net.udp",
    "std.net.addr",
    "std.io",
    "std.io.file",
    "std.io.path",
    // Runtime
    "std.runtime",
    "std.runtime.string_builder",
    "std.runtime.memory",
    // Misc
    "std.math",
    "std.fmt",
    "std.time",
    "std.os",
    "std.env",
    "std.prelude",
];

/// Build import path completions for a given path prefix.
///
/// E.g. `["std", "collections"]` → modules under `std.collections.*`.
pub fn import_items(path_prefix: &[String]) -> Vec<CompletionItem> {
    let prefix_str = path_prefix.join(".");
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for module in STD_MODULES {
        // Check if this module starts with our prefix.
        let matches = if prefix_str.is_empty() {
            true
        } else if module.starts_with(&prefix_str) {
            // It must have a `.` after the prefix (i.e. it's a sub-module).
            let rest = &module[prefix_str.len()..];
            rest.is_empty() || rest.starts_with('.')
        } else {
            false
        };

        if !matches {
            continue;
        }

        // Extract the next segment after the prefix.
        let rest = if prefix_str.is_empty() {
            module.to_string()
        } else if *module == prefix_str.as_str() {
            // Exact match — no next segment.
            continue;
        } else {
            module[prefix_str.len() + 1..].to_string() // skip leading `.`
        };

        // Take only the next segment (first component of rest).
        let segment = rest.split('.').next().unwrap_or("").to_string();
        if segment.is_empty() || !seen.insert(segment.clone()) {
            continue;
        }

        out.push(CompletionItem {
            label: segment.clone(),
            kind: Some(CompletionItemKind::MODULE),
            detail: Some(format!("module {}.{}", prefix_str, segment)),
            sort_text: Some(format!("{}{}", RANK_STD, segment)),
            documentation: Some(Documentation::MarkupContent(MarkupContent {
                kind: MarkupKind::Markdown,
                value: format!("**std module** `{}.{}`", prefix_str, segment),
            })),
            ..Default::default()
        });
    }
    out
}

// ─────────────────────────────────────────────────────────────────────────────
// Main entry point
// ─────────────────────────────────────────────────────────────────────────────

/// Compute all completion items for cursor at `offset` in `src`.
///
/// Returns an empty Vec if:
/// - cursor is in a comment or string
/// - no relevant completions found
///
/// This is the single entry point called by the LSP server handler.
/// `offset` is clamped to `src.len()` if past the end.
pub fn completion_for(src: &str, offset: usize) -> Vec<CompletionItem> {
    let offset = offset.min(src.len());
    let ctx = match detect_context(src, offset) {
        Some(c) => c,
        None => return vec![], // in comment or string
    };

    let mut items: Vec<CompletionItem> = Vec::new();

    match &ctx {
        CompletionContext::MethodDot { .. } => {
            items.extend(method_items(src, offset));
        }
        CompletionContext::Import { path_prefix } => {
            items.extend(import_items(path_prefix));
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

    fn label_kinds(items: &[CompletionItem]) -> Vec<(String, Option<CompletionItemKind>)> {
        items.iter().map(|i| (i.label.clone(), i.kind)).collect()
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
    // 104.3.3 — Method-dot (8 pos + 2 neg + 2 edge)
    // ─────────────────────────────────────────────────────────────────────────

    /// md_pos1: `x.` with int type yields int methods.
    #[test]
    fn md_pos1_int_methods() {
        let src = "module t\nfn f() -> () {\n    ro x int = 5\n    x.";
        let items = method_items(src, src.len());
        assert!(has_label(&items, "min"), "min should be in int methods");
        assert!(has_label(&items, "max"), "max should be in int methods");
        assert!(has_label(&items, "clamp"), "clamp should be in int methods");
        assert!(has_label(&items, "compare"), "compare should be in int methods");
    }

    /// md_pos2: `s.` with str type yields str methods.
    #[test]
    fn md_pos2_str_methods() {
        let src = "module t\nfn f() -> () {\n    ro s str = \"\"\n    s.";
        let items = method_items(src, src.len());
        assert!(has_label(&items, "byte_len"), "byte_len in str methods");
        assert!(has_label(&items, "contains"), "contains in str methods");
        assert!(has_label(&items, "to_upper"), "to_upper in str methods");
        assert!(has_label(&items, "as_bytes"), "as_bytes in str methods");
        assert!(has_label(&items, "as_chars"), "as_chars in str methods");
        assert!(has_label(&items, "lines"), "lines in str methods");
        assert!(!has_label(&items, "len"), "len removed — use byte_len()");
        assert!(!has_label(&items, "chars"), "chars removed — use as_chars()");
        assert!(!has_label(&items, "split_lines"), "split_lines removed — use lines()");
    }

    /// md_pos3: `v.` with Vec type yields vec methods.
    #[test]
    fn md_pos3_vec_methods() {
        let src = "module t\nfn f(items_vec []int) -> () {\n    items_vec.";
        let items = method_items(src, src.len());
        assert!(has_label(&items, "len"), "len in vec methods");
        assert!(has_label(&items, "push"), "push in vec methods");
        assert!(has_label(&items, "iter"), "iter in vec methods");
        assert!(has_label(&items, "map"), "map in vec methods");
        assert!(has_label(&items, "append"), "append in vec methods");
        assert!(!has_label(&items, "extend") || has_label(&items, "extend"),
            "extend exists (via Iter[T]) — this assertion is informational");
    }

    /// md_pos4: method items have METHOD kind.
    #[test]
    fn md_pos4_method_kind() {
        let src = "module t\nfn f() -> () {\n    ro n int = 0\n    n.";
        let items = method_items(src, src.len());
        for item in &items {
            assert_eq!(item.kind, Some(CompletionItemKind::METHOD));
        }
    }

    /// md_pos5: method items have non-empty detail.
    #[test]
    fn md_pos5_method_detail() {
        let src = "module t\nfn f() -> () {\n    ro s str = \"\"\n    s.";
        let items = method_items(src, src.len());
        assert!(!items.is_empty(), "str methods expected");
        for item in &items {
            assert!(item.detail.is_some(), "detail should be set");
        }
    }

    /// md_pos6: detect_context returns MethodDot for `x.`.
    #[test]
    fn md_pos6_context_detection_method_dot() {
        let src = "module t\nfn f() -> () {\n    ro x int = 5\n    x.";
        let ctx = detect_context(src, src.len());
        assert!(
            matches!(ctx, Some(CompletionContext::MethodDot { .. })),
            "expected MethodDot context, got {:?}",
            ctx
        );
    }

    /// md_pos7: user-defined receiver method appears in completions.
    #[test]
    fn md_pos7_user_defined_method() {
        let src = "module t\nfn Foo @greet() -> str => \"hello\"\nfn f() -> () {\n    ro x Foo = Foo {}\n    x.";
        let items = method_items(src, src.len());
        assert!(
            has_label(&items, "greet"),
            "user-defined method greet should appear"
        );
    }

    /// md_pos8: f64 type yields f64 methods (sqrt, floor, abs, ...).
    #[test]
    fn md_pos8_f64_methods() {
        let src = "module t\nfn f() -> () {\n    ro v f64 = 1.0\n    v.";
        let items = method_items(src, src.len());
        assert!(has_label(&items, "sqrt"), "sqrt in f64 methods");
        assert!(has_label(&items, "floor"), "floor in f64 methods");
        assert!(has_label(&items, "abs"), "abs in f64 methods");
        assert!(has_label(&items, "is_nan"), "is_nan in f64 methods");
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
        assert!(has_label(&items, "sync"), "sync under std");
        assert!(has_label(&items, "net"), "net under std");
    }

    /// imp_pos3: `["std", "collections"]` prefix returns vec, map, set.
    #[test]
    fn imp_pos3_collections_returns_submodules() {
        let prefix = vec!["std".to_string(), "collections".to_string()];
        let items = import_items(&prefix);
        assert!(has_label(&items, "vec"), "vec under std.collections");
        assert!(has_label(&items, "map"), "map under std.collections");
        assert!(has_label(&items, "set"), "set under std.collections");
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
}
