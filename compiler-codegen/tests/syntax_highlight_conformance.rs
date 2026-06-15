//! Conformance guard (D278): the editor syntax-highlighting keyword lists in
//! `editors/` MUST track the lexer keyword set — `compiler-codegen/src/lexer/mod.rs`
//! (`lex_ident_or_keyword`), the single source of truth.
//!
//! This test is anchored to the **live lexer**: it lexes every word and asserts the
//! resulting token kind, so the classification cannot rot independently of the
//! compiler. It then checks each in-repo highlighter for (a) absence of phantom
//! keywords (retired / non-keyword words highlighted as keywords) and (b) presence
//! of every active keyword (for the regex highlighters that can express them).
//!
//! Covered (regex-based, in-repo, fully fixable):
//!   - editors/vscode/syntaxes/nova.tmLanguage.json
//!   - editors/vim/syntax/nova.vim
//!   - editors/zed/languages/nova/highlights.scm   (tree-sitter → phantom-only check;
//!     completeness is gated on the external grammar github.com/nv-lang/tree-sitter-nova,
//!     tracked as [M-treesitter-grammar-keyword-bump])
//!
//! The website highlighter lives in the separate `www` repo and is guarded there by
//! `www/site/scripts/check-highlight-keywords.mjs` (run via `npm run check:highlight`).

use nova_codegen::lexer::{lex, TokenKind};
use std::fs;
use std::path::PathBuf;

/// ACTIVE reserved keywords — the lexer must produce a non-`Ident` keyword token.
/// Mirror of the `match` arms in `lex_ident_or_keyword` (verified by
/// `active_keywords_are_lexed_as_keywords`).
const ACTIVE: &[&str] = &[
    "module", "import", "use", "export", "external",
    "fn", "type", "effect", "alias", "protocol",
    "const", "mut", "consume", "ro", "priv", "pub", "unsafe",
    "if", "else", "match", "for", "while", "loop", "in", "return", "break", "continue",
    "test", "with", "throw", "as", "is",
    "spawn", "supervised", "parallel", "detach", "blocking", "interrupt",
    "forbid", "realtime", "defer", "errdefer", "okdefer", "select", "lemma",
    "true", "false",
];

/// RETIRED lexemes — tokenized ONLY so the parser can emit a precise "removed"
/// diagnostic (E_KW_REMOVED_LET / E_KW_REMOVED_READONLY / E_SAFE_RETIRED). They are
/// INVALID in the language → must NEVER be highlighted as keywords.
const RETIRED: &[&str] = &["let", "readonly", "safe"];

/// Words that are NOT keywords at all (the lexer yields `Ident`) → must NEVER be
/// highlighted as keywords. `handler` (D142) is now a plain identifier; Nova uses
/// `&&`/`||`/`!` so `and`/`or`/`not` are not keywords; `race`/`with_timeout`/
/// `cancel_scope`/`region` never were keywords.
const NON_KEYWORDS: &[&str] = &[
    "handler", "and", "or", "not", "race", "with_timeout", "cancel_scope", "region",
];

/// Phantom set = words that must not appear as highlighted keywords.
fn phantoms() -> Vec<&'static str> {
    RETIRED.iter().chain(NON_KEYWORDS.iter()).copied().collect()
}

fn lex_first_kind(word: &str) -> TokenKind {
    lex(word)
        .expect("lex must succeed")
        .into_iter()
        .map(|t| t.kind)
        .find(|k| !matches!(k, TokenKind::Newline | TokenKind::Eof))
        .expect("at least one token")
}

fn lexes_as_ident(word: &str) -> bool {
    matches!(lex_first_kind(word), TokenKind::Ident(_))
}

fn repo_root() -> PathBuf {
    // CARGO_MANIFEST_DIR = <repo>/compiler-codegen → parent = <repo>
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("compiler-codegen has a parent dir")
        .to_path_buf()
}

fn read_repo(rel: &str) -> String {
    let path = repo_root().join(rel);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// Word-boundary aware substring search (no regex crate in the dep tree). Matches
/// `word` only when bounded by non-identifier characters on both sides, so e.g.
/// `or` inside `import` and `let` inside `let_declaration` do not false-positive.
fn contains_word(haystack: &str, word: &str) -> bool {
    let bytes = haystack.as_bytes();
    let is_id = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    let mut from = 0usize;
    while let Some(rel) = haystack[from..].find(word) {
        let start = from + rel;
        let end = start + word.len();
        let before_ok = start == 0 || !is_id(bytes[start - 1]);
        let after_ok = end >= bytes.len() || !is_id(bytes[end]);
        if before_ok && after_ok {
            return true;
        }
        from = start + 1;
        if from >= bytes.len() {
            break;
        }
    }
    false
}

/// Drop full-line comments so words mentioned in comments are not mistaken for
/// highlighted keywords. `line_comment_prefixes` are matched against the trimmed
/// start of each line.
fn strip_comment_lines(src: &str, line_comment_prefixes: &[&str]) -> String {
    src.lines()
        .filter(|l| {
            let t = l.trim_start();
            !line_comment_prefixes.iter().any(|p| t.starts_with(p))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Keep only the lines that actually define keyword groups (used for vim, whose
/// keyword corpus is exactly the `syntax keyword nova*` lines).
fn keep_lines_starting_with(src: &str, prefix: &str) -> String {
    src.lines()
        .filter(|l| l.trim_start().starts_with(prefix))
        .collect::<Vec<_>>()
        .join("\n")
}

// ─── 1. Anchor the three keyword classes to the LIVE lexer ───────────────────

#[test]
fn active_keywords_are_lexed_as_keywords() {
    for &kw in ACTIVE {
        assert!(
            !lexes_as_ident(kw),
            "`{kw}` is listed as ACTIVE but the lexer produces an identifier — \
             it is not actually a keyword (check lex_ident_or_keyword)"
        );
    }
}

#[test]
fn retired_keywords_still_tokenize_but_are_listed_retired() {
    // Retired lexemes keep a dedicated Kw* token so the parser can emit a precise
    // diagnostic; they must not be treated as identifiers (that would mean the
    // retraction machinery was removed and this list is stale).
    for &kw in RETIRED {
        assert!(
            !lexes_as_ident(kw),
            "`{kw}` no longer tokenizes to a retired keyword token — update RETIRED"
        );
    }
}

#[test]
fn non_keywords_are_lexed_as_identifiers() {
    for &w in NON_KEYWORDS {
        assert!(
            lexes_as_ident(w),
            "`{w}` is listed as a non-keyword but the lexer treats it as a keyword — \
             update the list (and the highlighters)"
        );
    }
}

// ─── 2. VSCode TextMate (pure JSON, no comments) ─────────────────────────────

#[test]
fn vscode_grammar_has_no_phantom_keywords() {
    let src = read_repo("editors/vscode/syntaxes/nova.tmLanguage.json");
    for w in phantoms() {
        assert!(
            !contains_word(&src, w),
            "VSCode tmLanguage still highlights phantom keyword `{w}`"
        );
    }
}

#[test]
fn vscode_grammar_has_all_active_keywords() {
    let src = read_repo("editors/vscode/syntaxes/nova.tmLanguage.json");
    for &kw in ACTIVE {
        assert!(
            contains_word(&src, kw),
            "VSCode tmLanguage is missing active keyword `{kw}`"
        );
    }
}

// ─── 3. Vim syntax ───────────────────────────────────────────────────────────

#[test]
fn vim_syntax_has_no_phantom_keywords() {
    // Corpus = only the `syntax keyword nova*` lines (ignores `"` comments).
    let corpus = keep_lines_starting_with(
        &read_repo("editors/vim/syntax/nova.vim"),
        "syntax keyword",
    );
    for w in phantoms() {
        assert!(
            !contains_word(&corpus, w),
            "Vim syntax still highlights phantom keyword `{w}`"
        );
    }
}

#[test]
fn vim_syntax_has_all_active_keywords() {
    let corpus = keep_lines_starting_with(
        &read_repo("editors/vim/syntax/nova.vim"),
        "syntax keyword",
    );
    for &kw in ACTIVE {
        assert!(
            contains_word(&corpus, kw),
            "Vim syntax is missing active keyword `{kw}`"
        );
    }
}

// ─── 4. Zed tree-sitter query (phantom-only; completeness is grammar-gated) ───

#[test]
fn zed_query_has_no_phantom_keywords() {
    // Drop `;`-comment lines (they intentionally mention removed words).
    let body = strip_comment_lines(
        &read_repo("editors/zed/languages/nova/highlights.scm"),
        &[";"],
    );
    for w in phantoms() {
        assert!(
            !contains_word(&body, w),
            "Zed highlights.scm still references phantom keyword `{w}` outside comments"
        );
    }
}
