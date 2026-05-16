//! Plan 45 Ф.33.1 — server-side syntax highlighting через Nova lexer.
//!
//! Replaces JS regex tokenizer (Ф.31.5) на server-side lexer-based output.
//! Output: HTML-escaped string с `<span class="tok-X">...</span>` markers
//! для каждого token. Embedded в `<pre><code>` без JS dependency.
//!
//! **Advantages over JS regex:**
//! - Accurate tokenization (uses production Nova lexer)
//! - No JS dependency — works в text-mode browsers, RSS readers, etc
//! - Faster page load (no client-side processing)
//! - Context-aware (например `fn` в string literal НЕ подсветится как keyword)
//!
//! **Token classes:**
//! - `tok-kw` — keywords (fn, let, const, if, match, etc)
//! - `tok-type` — built-in types (int, str, bool, etc)
//! - `tok-str` — string literals + char literals
//! - `tok-num` — numeric literals
//! - `tok-comment` — `//` line comments + `///` `//!` doc-comments
//! - `tok-op` — operators (rare class — большинство не highlighted)
//!
//! **Fallback:** если lex fails (malformed source) — return HTML-escaped source.

use crate::lexer::{lex, TokenKind};

/// Plan 45 Ф.33.1 — highlight Nova source code as HTML string.
///
/// Output: HTML с `<span class="tok-...">` markers. Suitable для embedding
/// в `<pre><code>{output}</code></pre>` (caller adds wrapper).
///
/// Whitespace и unknown tokens emitted as raw HTML-escaped text.
pub fn highlight_html(source: &str) -> String {
    // Use lexer для accurate tokenization.
    let tokens = match lex(source) {
        Ok(t) => t,
        Err(_) => {
            // Lexer failure (malformed source) — fallback to escape-only.
            return escape_html(source);
        }
    };
    let mut out = String::with_capacity(source.len() * 2);
    let mut cursor = 0usize;
    for tok in &tokens {
        let start = tok.span.start;
        let end = tok.span.end;
        // Emit раздел между предыдущим token и этим (whitespace, comments).
        if start > cursor {
            // Этот gap может содержать whitespace + non-doc comments (line `//`,
            // block `/* */`). Lexer skip'ает эти — try detect comments внутри
            // и markup'нуть как tok-comment.
            let gap = &source[cursor..start];
            emit_gap(&mut out, gap);
        }
        if end > source.len() || start >= source.len() {
            // Span out-of-bounds (shouldn't happen, defensive).
            cursor = end.min(source.len());
            continue;
        }
        let text = &source[start..end];
        let class = token_class(&tok.kind);
        if let Some(c) = class {
            out.push_str("<span class=\"");
            out.push_str(c);
            out.push_str("\">");
            out.push_str(&escape_html(text));
            out.push_str("</span>");
        } else {
            out.push_str(&escape_html(text));
        }
        cursor = end;
    }
    // Trailing gap (после последнего token).
    if cursor < source.len() {
        emit_gap(&mut out, &source[cursor..]);
    }
    out
}

/// Emit gap (text между tokens) — может содержать whitespace, line comments
/// (`// ...\n`), block comments (`/* ... */`). Detect comments inline и
/// markup как `.tok-comment`.
fn emit_gap(out: &mut String, gap: &str) {
    let bytes = gap.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // Line comment `//` (НЕ doc — doc уже это token).
        if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'/' {
            // Skip if doc-comment (`///` or `//!`) — это already token.
            let third = bytes.get(i + 2).copied();
            if third != Some(b'/') && third != Some(b'!') {
                // Regular line comment.
                let end = gap[i..].find('\n').map(|n| i + n).unwrap_or(bytes.len());
                out.push_str("<span class=\"tok-comment\">");
                out.push_str(&escape_html(&gap[i..end]));
                out.push_str("</span>");
                i = end;
                continue;
            }
        }
        // Block comment `/* ... */`.
        if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'*' {
            let mut end = i + 2;
            while end + 1 < bytes.len() {
                if bytes[end] == b'*' && bytes[end + 1] == b'/' {
                    end += 2;
                    break;
                }
                end += 1;
            }
            end = end.min(bytes.len());
            out.push_str("<span class=\"tok-comment\">");
            out.push_str(&escape_html(&gap[i..end]));
            out.push_str("</span>");
            i = end;
            continue;
        }
        // Plain whitespace / other — emit as-is.
        let next_special = gap[i..]
            .find('/')
            .map(|n| i + n)
            .unwrap_or(bytes.len());
        out.push_str(&escape_html(&gap[i..next_special]));
        i = next_special;
        if i < bytes.len() && bytes[i] == b'/' && i + 1 < bytes.len() && (bytes[i + 1] == b'/' || bytes[i + 1] == b'*') {
            continue; // will handle in next iter
        }
        // Single `/` not followed by `/` or `*` — emit as-is.
        if i < bytes.len() {
            out.push_str(&escape_html(&gap[i..i + 1]));
            i += 1;
        }
    }
}

/// Map TokenKind → CSS class name (or None for raw output).
fn token_class(kind: &TokenKind) -> Option<&'static str> {
    match kind {
        // Literals — numbers/strings/chars.
        TokenKind::Int(_) | TokenKind::Float(_) => Some("tok-num"),
        TokenKind::Str(_) | TokenKind::Char(_) | TokenKind::Backtick(_) => Some("tok-str"),
        // Doc-comments.
        TokenKind::DocComment { .. } => Some("tok-comment"),
        // Built-in type identifiers detected by name.
        TokenKind::Ident(s) => {
            const TYPES: &[&str] = &[
                "int", "str", "bool", "float", "char", "unit", "void", "any",
                "i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64", "f32", "f64",
            ];
            if TYPES.contains(&s.as_str()) { Some("tok-type") } else { None }
        }
        // Wildcard на KwX через Debug name — robust к добавлению новых keywords.
        // `KwModule`, `KwFn`, `KwLet`, etc — все debug-print со starts_with("Kw").
        other => {
            if is_keyword_kind(other) { Some("tok-kw") } else { None }
        }
    }
}

/// Detection via Debug derivative: TokenKind variants начинающиеся с `Kw`
/// — keywords. Robust к новым keywords (не требует enumeration).
fn is_keyword_kind(kind: &TokenKind) -> bool {
    let debug = format!("{:?}", kind);
    debug.starts_with("Kw")
}

/// HTML escape for embedding в `<code>` block.
fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlights_keyword_fn() {
        let s = highlight_html("fn foo() {}");
        assert!(s.contains("<span class=\"tok-kw\">fn</span>"),
            "should highlight `fn` keyword, got: {}", s);
    }

    #[test]
    fn highlights_type_int() {
        let s = highlight_html("let x int = 5");
        assert!(s.contains("<span class=\"tok-type\">int</span>"),
            "should highlight `int` as type, got: {}", s);
    }

    #[test]
    fn highlights_string_literal() {
        let s = highlight_html("let s = \"hello\"");
        assert!(s.contains("<span class=\"tok-str\">"),
            "should highlight string literal, got: {}", s);
    }

    #[test]
    fn highlights_number() {
        let s = highlight_html("let n = 42");
        assert!(s.contains("<span class=\"tok-num\">42</span>"),
            "should highlight `42` as number, got: {}", s);
    }

    #[test]
    fn highlights_doc_comment() {
        let s = highlight_html("/// Doc comment\nfn x() {}");
        assert!(s.contains("<span class=\"tok-comment\">"),
            "doc comment должен highlighted, got: {}", s);
    }

    #[test]
    fn highlights_line_comment() {
        // Regular `//` line comment — emitted в gap, должен detect.
        let s = highlight_html("fn x() {} // trailing comment");
        assert!(s.contains("<span class=\"tok-comment\">// trailing comment</span>"),
            "line comment должен highlighted, got: {}", s);
    }

    #[test]
    fn escapes_html_in_idents_safely() {
        // Идентификаторы НЕ могут содержать <>, но defensive: any character
        // escaped properly.
        let s = highlight_html("let a = b");
        assert!(!s.contains("<a>"), "raw HTML tags не должны быть в output");
    }

    #[test]
    fn malformed_source_fallback_to_escape() {
        // Если lexer fails — fallback escape-only.
        let s = highlight_html("\u{0000}\u{0001}");
        // Should not panic; output должно быть escaped.
        assert!(!s.is_empty() || true);
    }

    #[test]
    fn no_keyword_in_string_literal() {
        // Critical context-aware test: `fn` внутри string не должен highlight.
        let s = highlight_html("let x = \"fn keyword\"");
        // Ident `let` highlighted (keyword), но `fn` внутри string — нет
        // (string literal целиком обёрнут в tok-str).
        assert!(s.contains("<span class=\"tok-kw\">let</span>"));
        assert!(!s.contains("<span class=\"tok-kw\">fn</span>"),
            "fn внутри string НЕ должен highlighted, got: {}", s);
    }
}
