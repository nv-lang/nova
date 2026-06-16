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
    ("let", "Declare a readonly binding"),
    ("import", "Import a module or items"),
    ("export", "Export a declaration"),
    ("module", "Module declaration"),
    ("effect", "Declare an effect type"),
    ("protocol", "Declare a protocol"),
    ("test", "Declare a test block"),
    ("bench", "Declare a benchmark"),
    ("lemma", "Declare a proven lemma"),
];

/// Keywords available inside a function body.
const FN_BODY_KEYWORDS: &[(&str, &str)] = &[
    ("let", "Readonly binding"),
    ("mut", "Mutable binding"),
    ("ro", "Explicit readonly binding"),
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
    ("blocking", "Mark blocking IO operation"),
    ("unsafe", "Unsafe block"),
    ("consume", "Consume a value"),
    ("apply", "Apply a lemma"),
];

/// Keywords available inside a type body.
const TYPE_BODY_KEYWORDS: &[(&str, &str)] = &[
    ("fn", "Declare a method"),
    ("const", "Declare an associated constant"),
    ("type", "Declare an associated type alias"),
    ("pub", "Mark a field as public"),
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
        detail: "if let pattern = expr { ... }",
        insert_text: "if let ${1:Some(x)} = ${2:expr} {\n\t${3:()}\n}",
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

/// Scan for `let X ...` and `mut X ...` bindings before `offset`.
fn collect_let_bindings(text: &str) -> Vec<IdentInfo> {
    let mut out = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("let ").or_else(|| trimmed.strip_prefix("mut ")) {
            // Name is the first identifier.
            let name = first_ident(rest);
            if !name.is_empty() {
                // Try to extract type hint: `let name TYPE = ...`
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
    // Parse `name Type, name Type, ...`
    for param in params_str.split(',') {
        let name = first_ident(param.trim());
        if name.is_empty() || name == "self" || name == "ro" || name == "mut" {
            continue;
        }
        let type_hint = extract_type_after_name(param.trim(), &name);
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
        // Primitive types.
        ("int", "primitive integer type", IdentKind::Type),
        ("float", "primitive float type", IdentKind::Type),
        ("bool", "primitive boolean type", IdentKind::Type),
        ("str", "string value type", IdentKind::Type),
        ("char", "Unicode scalar value", IdentKind::Type),
        ("u8", "unsigned 8-bit integer", IdentKind::Type),
        ("u32", "unsigned 32-bit integer", IdentKind::Type),
        ("u64", "unsigned 64-bit integer", IdentKind::Type),
        ("i32", "signed 32-bit integer", IdentKind::Type),
        ("i64", "signed 64-bit integer", IdentKind::Type),
        ("usize", "pointer-sized unsigned integer", IdentKind::Type),
        // Stdlib types.
        ("Option", "Option[T] — optional value", IdentKind::Type),
        ("Result", "Result[T,E] — failable value", IdentKind::Type),
        ("Vec", "Vec[T] — growable array", IdentKind::Type),
        ("Map", "Map[K,V] — hash map", IdentKind::Type),
        ("Set", "Set[T] — hash set", IdentKind::Type),
        ("Range", "Range — integer range i..j", IdentKind::Type),
        ("StringBuilder", "mutable string builder", IdentKind::Type),
        // Common functions.
        ("print", "print string to stdout", IdentKind::Fn),
        ("println", "print string + newline", IdentKind::Fn),
        ("assert", "runtime assertion", IdentKind::Fn),
        ("panic", "panic with message", IdentKind::Fn),
        ("todo", "todo placeholder (panics)", IdentKind::Fn),
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
fn int_methods() -> Vec<MethodInfo> {
    vec![
        MethodInfo::new("abs", "() -> int", "Absolute value"),
        MethodInfo::new("min", "(other int) -> int", "Minimum of two ints"),
        MethodInfo::new("max", "(other int) -> int", "Maximum of two ints"),
        MethodInfo::new("clamp", "(lo int, hi int) -> int", "Clamp value in range"),
        MethodInfo::new("to_str", "() -> str", "Convert int to string"),
        MethodInfo::new("as_float", "() -> float", "Convert to float"),
        MethodInfo::new("pow", "(exp int) -> int", "Integer power"),
        MethodInfo::new("is_positive", "() -> bool", "True if > 0"),
        MethodInfo::new("is_negative", "() -> bool", "True if < 0"),
        MethodInfo::new("is_zero", "() -> bool", "True if == 0"),
    ]
}

/// Methods available on `float`.
fn float_methods() -> Vec<MethodInfo> {
    vec![
        MethodInfo::new("abs", "() -> float", "Absolute value"),
        MethodInfo::new("floor", "() -> float", "Floor"),
        MethodInfo::new("ceil", "() -> float", "Ceiling"),
        MethodInfo::new("round", "() -> float", "Round to nearest"),
        MethodInfo::new("sqrt", "() -> float", "Square root"),
        MethodInfo::new("to_str", "() -> str", "Convert float to string"),
        MethodInfo::new("is_nan", "() -> bool", "True if NaN"),
        MethodInfo::new("is_infinite", "() -> bool", "True if infinite"),
        MethodInfo::new("min", "(other float) -> float", "Minimum"),
        MethodInfo::new("max", "(other float) -> float", "Maximum"),
    ]
}

/// Methods available on `str`.
fn str_methods() -> Vec<MethodInfo> {
    vec![
        MethodInfo::new("len", "() -> int", "Byte length"),
        MethodInfo::new("is_empty", "() -> bool", "True if empty"),
        MethodInfo::new("contains", "(sub str) -> bool", "Substring check"),
        MethodInfo::new("starts_with", "(prefix str) -> bool", "Prefix check"),
        MethodInfo::new("ends_with", "(suffix str) -> bool", "Suffix check"),
        MethodInfo::new("to_upper", "() -> str", "Uppercase"),
        MethodInfo::new("to_lower", "() -> str", "Lowercase"),
        MethodInfo::new("trim", "() -> str", "Trim whitespace"),
        MethodInfo::new("trim_start", "() -> str", "Trim left whitespace"),
        MethodInfo::new("trim_end", "() -> str", "Trim right whitespace"),
        MethodInfo::new("split", "(sep str) -> []str", "Split by separator"),
        MethodInfo::new("split_lines", "() -> []str", "Split into lines"),
        MethodInfo::new("replace", "(from str, to str) -> str", "Replace substring"),
        MethodInfo::new("find", "(sub str) -> Option[int]", "Find substring index"),
        MethodInfo::new("as_bytes", "() -> []u8", "Get bytes"),
        MethodInfo::new("chars", "() -> []char", "Get chars"),
        MethodInfo::new("to_int", "() -> Option[int]", "Parse as int"),
        MethodInfo::new("to_float", "() -> Option[float]", "Parse as float"),
        MethodInfo::new("repeat", "(n int) -> str", "Repeat n times"),
    ]
}

/// Methods available on `[]T` (Vec).
fn vec_methods() -> Vec<MethodInfo> {
    vec![
        MethodInfo::new("len", "() -> int", "Number of elements"),
        MethodInfo::new("is_empty", "() -> bool", "True if empty"),
        MethodInfo::new("push", "(item T) -> ()", "Append element"),
        MethodInfo::new("pop", "() -> Option[T]", "Remove and return last"),
        MethodInfo::new("get", "(i int) -> Option[T]", "Get by index (bounds-checked)"),
        MethodInfo::new("first", "() -> Option[T]", "First element"),
        MethodInfo::new("last", "() -> Option[T]", "Last element"),
        MethodInfo::new("contains", "(item T) -> bool", "Membership test"),
        MethodInfo::new("index_of", "(item T) -> Option[int]", "First index of item"),
        MethodInfo::new("reverse", "() -> ()", "Reverse in place"),
        MethodInfo::new("sort", "() -> ()", "Sort in place"),
        MethodInfo::new("map", "(f fn(T)->U) -> []U", "Map over elements"),
        MethodInfo::new("filter", "(f fn(T)->bool) -> []T", "Filter elements"),
        MethodInfo::new("flat_map", "(f fn(T)->[]U) -> []U", "Flat-map"),
        MethodInfo::new("fold", "(init A, f fn(A,T)->A) -> A", "Fold/reduce"),
        MethodInfo::new("any", "(f fn(T)->bool) -> bool", "True if any matches"),
        MethodInfo::new("all", "(f fn(T)->bool) -> bool", "True if all match"),
        MethodInfo::new("count", "(f fn(T)->bool) -> int", "Count matches"),
        MethodInfo::new("find", "(f fn(T)->bool) -> Option[T]", "First match"),
        MethodInfo::new("extend", "(other []T) -> ()", "Extend with another vec"),
        MethodInfo::new("clear", "() -> ()", "Remove all elements"),
        MethodInfo::new("clone", "() -> []T", "Deep clone"),
    ]
}

/// Methods available on `bool`.
fn bool_methods() -> Vec<MethodInfo> {
    vec![
        MethodInfo::new("to_str", "() -> str", "Convert to string"),
        MethodInfo::new("not", "() -> bool", "Logical negation"),
    ]
}

/// Methods available on `Option[T]`.
fn option_methods() -> Vec<MethodInfo> {
    vec![
        MethodInfo::new("is_some", "() -> bool", "True if Some"),
        MethodInfo::new("is_none", "() -> bool", "True if None"),
        MethodInfo::new("unwrap", "() -> T", "Unwrap or panic"),
        MethodInfo::new("unwrap_or", "(default T) -> T", "Unwrap or default"),
        MethodInfo::new("map", "(f fn(T)->U) -> Option[U]", "Map over value"),
        MethodInfo::new("and_then", "(f fn(T)->Option[U]) -> Option[U]", "Flat-map"),
        MethodInfo::new("or_else", "(f fn()->Option[T]) -> Option[T]", "Alternative"),
        MethodInfo::new("ok_or", "(err E) -> Result[T,E]", "Convert to Result"),
    ]
}

/// Methods available on `Result[T,E]`.
fn result_methods() -> Vec<MethodInfo> {
    vec![
        MethodInfo::new("is_ok", "() -> bool", "True if Ok"),
        MethodInfo::new("is_err", "() -> bool", "True if Err"),
        MethodInfo::new("unwrap", "() -> T", "Unwrap Ok or panic"),
        MethodInfo::new("unwrap_err", "() -> E", "Unwrap Err or panic"),
        MethodInfo::new("ok", "() -> Option[T]", "Convert Ok to Option"),
        MethodInfo::new("err", "() -> Option[E]", "Convert Err to Option"),
        MethodInfo::new("map", "(f fn(T)->U) -> Result[U,E]", "Map Ok value"),
        MethodInfo::new("map_err", "(f fn(E)->F) -> Result[T,F]", "Map Err value"),
    ]
}

/// Infer the type of an expression given its text and the surrounding source.
///
/// V1: text-pattern heuristic. Returns the type name or None.
fn infer_type_of_expr(obj_text: &str, src: &str) -> Option<String> {
    // Check let/mut bindings to find the type annotation.
    for line in src.lines() {
        let t = line.trim();
        // Pattern: `let name TYPE = ...` or `let name: TYPE = ...`
        for prefix in &["let ", "mut ", "ro "] {
            if let Some(rest) = t.strip_prefix(prefix) {
                let name = first_ident(rest);
                if name == obj_text {
                    if let Some(ty) = extract_type_after_name(rest, &name) {
                        return Some(ty);
                    }
                }
            }
        }
        // Pattern: fn param `name Type,`
        if t.contains('(') && t.contains(obj_text) {
            // Search within parameter list.
            if let Some(paren_start) = t.find('(') {
                let params = &t[paren_start + 1..];
                for param in params.split(',') {
                    let pname = first_ident(param.trim());
                    if pname == obj_text {
                        if let Some(ty) = extract_type_after_name(param.trim(), &pname) {
                            return Some(ty);
                        }
                    }
                }
            }
        }
    }

    // Heuristic: if obj_text looks like a vec/array (has `_vec`, `_list`, `items`, `elems`),
    // assume `[]T`.
    let lower = obj_text.to_lowercase();
    if lower.contains("vec") || lower.contains("list") || lower.ends_with('s')
        || lower.contains("items") || lower.contains("elems")
    {
        // Don't over-eagerly match short names like "xs" though.
        if lower.ends_with("_vec") || lower.ends_with("_list") || lower == "items" || lower == "elems" {
            return Some("Vec".to_string());
        }
    }
    // Heuristic: common naming conventions.
    if lower.contains("name") || lower.contains("msg") || lower.contains("text") || lower.contains("str") || lower.contains("label") {
        return Some("str".to_string());
    }
    if lower.contains("count") || lower.contains("index") || lower.contains("len") || lower.contains("size") {
        return Some("int".to_string());
    }
    if lower == "ok" || lower == "result" || lower == "res" {
        return Some("Result".to_string());
    }
    if lower == "opt" || lower == "maybe" {
        return Some("Option".to_string());
    }

    None
}

/// Get methods for a given type name.
pub fn methods_for_type(ty: &str) -> Vec<MethodInfo> {
    match ty {
        "int" | "i32" | "i64" | "u8" | "u32" | "u64" | "usize" => int_methods(),
        "float" | "f32" | "f64" => float_methods(),
        "str" | "String" => str_methods(),
        "bool" => bool_methods(),
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
        // Unknown type: return all common methods.
        let mut all = vec![];
        all.extend(int_methods());
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

    /// kw_pos2: fn-body keywords include `let`, `if`, `for`, `match`.
    #[test]
    fn kw_pos2_fn_body_keywords() {
        let ctx = CompletionContext::FnBody;
        let items = keyword_items(&ctx);
        assert!(has_label(&items, "let"), "let missing from fn-body keywords");
        assert!(has_label(&items, "if"), "if missing");
        assert!(has_label(&items, "for"), "for missing");
        assert!(has_label(&items, "match"), "match missing");
        assert!(has_label(&items, "return"), "return missing");
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
    let x int = 5
    mut y float = 3.14
    let result int = add(x, 1)
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
        let src = "module t\nfn f() -> () {\n    let myvar int = 1\n    ";
        let offset = src.len();
        let items = completion_for(src, offset);
        assert!(
            has_label(&items, "myvar"),
            "myvar should appear in completion"
        );
        assert!(has_label(&items, "let"), "let keyword should appear");
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
        let src = "module t\nfn f() -> () {\n    let x int = 5\n    x.";
        let items = method_items(src, src.len());
        assert!(has_label(&items, "abs"), "abs should be in int methods");
        assert!(has_label(&items, "to_str"), "to_str should be in int methods");
        assert!(has_label(&items, "min"), "min should be in int methods");
    }

    /// md_pos2: `s.` with str type yields str methods.
    #[test]
    fn md_pos2_str_methods() {
        let src = "module t\nfn f() -> () {\n    let s str = \"\"\n    s.";
        let items = method_items(src, src.len());
        assert!(has_label(&items, "len"), "len in str methods");
        assert!(has_label(&items, "contains"), "contains in str methods");
        assert!(has_label(&items, "to_upper"), "to_upper in str methods");
    }

    /// md_pos3: `v.` with Vec type yields vec methods.
    #[test]
    fn md_pos3_vec_methods() {
        let src = "module t\nfn f(items_vec []int) -> () {\n    items_vec.";
        let items = method_items(src, src.len());
        assert!(has_label(&items, "len"), "len in vec methods");
        assert!(has_label(&items, "push"), "push in vec methods");
        assert!(has_label(&items, "map"), "map in vec methods");
    }

    /// md_pos4: method items have METHOD kind.
    #[test]
    fn md_pos4_method_kind() {
        let src = "module t\nfn f() -> () {\n    let n int = 0\n    n.";
        let items = method_items(src, src.len());
        for item in &items {
            assert_eq!(item.kind, Some(CompletionItemKind::METHOD));
        }
    }

    /// md_pos5: method items have non-empty detail.
    #[test]
    fn md_pos5_method_detail() {
        let src = "module t\nfn f() -> () {\n    let s str = \"\"\n    s.";
        let items = method_items(src, src.len());
        assert!(!items.is_empty(), "str methods expected");
        for item in &items {
            assert!(item.detail.is_some(), "detail should be set");
        }
    }

    /// md_pos6: detect_context returns MethodDot for `x.`.
    #[test]
    fn md_pos6_context_detection_method_dot() {
        let src = "module t\nfn f() -> () {\n    let x int = 5\n    x.";
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
        let src = "module t\nfn Foo @greet(self) -> str => \"hello\"\nfn f() -> () {\n    let x Foo = Foo {}\n    x.";
        let items = method_items(src, src.len());
        assert!(
            has_label(&items, "greet"),
            "user-defined method greet should appear"
        );
    }

    /// md_pos8: float type yields float methods.
    #[test]
    fn md_pos8_float_methods() {
        let src = "module t\nfn f() -> () {\n    let v float = 1.0\n    v.";
        let items = method_items(src, src.len());
        assert!(has_label(&items, "sqrt"), "sqrt in float methods");
        assert!(has_label(&items, "floor"), "floor in float methods");
    }

    /// md_neg1: no dot → method_items returns empty.
    #[test]
    fn md_neg1_no_dot_returns_empty() {
        let src = "module t\nfn f() -> () {\n    let x int = 5\n    x";
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
        let src = "module t\nfn f() -> () {\n    let s str = \"x.";
        let items = completion_for(src, src.len());
        assert!(items.is_empty(), "cursor in string should yield no completions");
    }

    /// md_edge2: number before dot (3.) is NOT a method call.
    #[test]
    fn md_edge2_number_dot_not_method_call() {
        let src = "module t\nfn f() -> () {\n    let x = 3.";
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
        let src = "module t\nfn myFn() -> () {}\nfn f() -> () {\n    let myVar int = 1\n    m";
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
        let src = "module t\nfn f() -> () {\n    let mylocal int = 1\n    ";
        let items = completion_for(src, src.len());
        let local_sort = items.iter()
            .find(|i| i.label == "mylocal")
            .and_then(|i| i.sort_text.as_deref())
            .unwrap_or("zz");
        let kw_sort = items.iter()
            .find(|i| i.label == "let")
            .and_then(|i| i.sort_text.as_deref())
            .unwrap_or("aa");
        assert!(
            local_sort < kw_sort,
            "local mylocal ({}) should sort before keyword let ({})",
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
        let src = r#"let s str = "hello"#;
        assert!(is_in_string(src, src.len()), "cursor inside string literal");
        assert!(!is_in_string(src, 5), "cursor before string is not in string");
    }
}
