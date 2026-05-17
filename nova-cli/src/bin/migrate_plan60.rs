//! Plan 60 Ф.2: lexer-based migration tool — переписывает field-style
//! size-accessors в method-form для всего corpus'а.
//!
//! Rewrite rules (D117):
//!   `expr.len`        → `expr.len()`
//!   `expr.is_empty`   → `expr.is_empty()`
//!   `expr.byte_len`   → `expr.byte_len()`
//!   `expr.cap`        → `expr.capacity()`   ← rename + append parens
//!   `expr.capacity`   → `expr.capacity()`   ← append parens
//!
//! Skip conditions (method-value form, legitimate):
//!   - предыдущий **значимый** token == `=`  (assignment: `let f = arr.len`)
//!   - предыдущий значимый token == `,` AND следующий уровень token-context
//!     указывает на fn-arg-position с expected `fn() -> T` (для bootstrap
//!     not tracked — будем conservative: НЕ skip, пусть пользователь сам
//!     обработает через annotation если ему нужен method-value в arg-position;
//!     edge-cases в stdlib не существуют).
//!
//! Token-level rewrite сохраняет comments / whitespace / formatting:
//! на каждый span original'а копируется bytes 1:1, кроме target tokens.
//!
//! Modes:
//!   --dry-run         только показать diff, не писать.
//!   --apply           реально записать (default false).
//!   --md              включить .md файлы (rewrite внутри ```nova / ```nv code blocks).
//!   --paths <dirs>    список директорий (default: std/ nova_tests/ examples/).
//!
//! После запуска `nova test` должен PASS (field-path всё ещё активен —
//! method-form rewrite не breaks).

use anyhow::{Context, Result};
use nova_codegen::lexer::{lex, Token, TokenKind};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug)]
struct Opts {
    apply: bool,
    include_md: bool,
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut opts = Opts { apply: false, include_md: false };
    let mut paths: Vec<PathBuf> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--apply" => opts.apply = true,
            "--dry-run" => opts.apply = false,
            "--md" => opts.include_md = true,
            "--paths" => {
                while i + 1 < args.len() && !args[i + 1].starts_with("--") {
                    paths.push(PathBuf::from(&args[i + 1]));
                    i += 1;
                }
            }
            other => {
                eprintln!("unknown arg: {}", other);
                eprintln!("usage: migrate_plan60 [--apply] [--dry-run] [--md] [--paths DIR ...]");
                std::process::exit(2);
            }
        }
        i += 1;
    }
    if paths.is_empty() {
        paths = vec!["std".into(), "nova_tests".into(), "examples".into()];
    }

    let mut total_files = 0usize;
    let mut total_rewrites = 0usize;
    let mut touched_files = 0usize;

    for dir in &paths {
        if !dir.exists() {
            eprintln!("skip: {} not found", dir.display());
            continue;
        }
        let files = collect_files(dir, opts.include_md)?;
        for f in files {
            total_files += 1;
            let src = std::fs::read_to_string(&f)
                .with_context(|| format!("read {}", f.display()))?;
            let rewritten = if f.extension().and_then(|s| s.to_str()) == Some("md") {
                rewrite_markdown(&src)?
            } else {
                rewrite_nova(&src)?
            };
            if rewritten.text != src {
                touched_files += 1;
                total_rewrites += rewritten.changes;
                println!("{}: {} change(s)", f.display(), rewritten.changes);
                if opts.apply {
                    std::fs::write(&f, &rewritten.text)
                        .with_context(|| format!("write {}", f.display()))?;
                }
            }
        }
    }

    println!();
    println!("=== Summary ===");
    println!("Files scanned : {}", total_files);
    println!("Files changed : {}", touched_files);
    println!("Total rewrites: {}", total_rewrites);
    if !opts.apply {
        println!("(dry-run — use --apply to actually write)");
    }

    Ok(())
}

fn collect_files(dir: &Path, include_md: bool) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    walk(dir, &mut out, include_md)?;
    out.sort();
    Ok(out)
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>, include_md: bool) -> Result<()> {
    for entry in std::fs::read_dir(dir).with_context(|| format!("read_dir {}", dir.display()))? {
        let entry = entry?;
        let p = entry.path();
        if p.is_dir() {
            // skip emitted artifacts dirs
            let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if name == "target" || name == ".git" || name == "node_modules" {
                continue;
            }
            walk(&p, out, include_md)?;
        } else if let Some(ext) = p.extension().and_then(|s| s.to_str()) {
            if ext == "nv" {
                out.push(p);
            } else if include_md && ext == "md" {
                out.push(p);
            }
        }
    }
    Ok(())
}

struct RewriteResult {
    text: String,
    changes: usize,
}

/// Token-level rewrite для Nova-source: lex, найти `Dot Ident(<accessor>)`
/// без trailing `LParen`, выдать modified source с insert/rename.
///
/// Сохраняет formatting через byte-copy между spans.
fn rewrite_nova(src: &str) -> Result<RewriteResult> {
    // lex может упасть на syntax-error; в этом случае возвращаем оригинал
    // (мы не хотим ломать сборку tool'ом для broken файлов — пользователь
    // увидит warning'и при сборке).
    let tokens = match lex(src) {
        Ok(t) => t,
        Err(_) => {
            return Ok(RewriteResult { text: src.to_string(), changes: 0 });
        }
    };
    rewrite_tokens(src, &tokens)
}

/// Rewrite source by tokens. Output assembled байтами оригинала + token replacements.
fn rewrite_tokens(src: &str, tokens: &[Token]) -> Result<RewriteResult> {
    let mut out = String::with_capacity(src.len() + 64);
    let mut cursor: usize = 0;
    let mut changes = 0usize;

    // Pre-compute prev significant token index for each i (skip Newline).
    // For Ф.2 we need to look behind to detect `let f = arr.len` skip-case.
    let is_significant = |k: &TokenKind| !matches!(k, TokenKind::Newline | TokenKind::Eof);

    let mut prev_sig: Vec<Option<usize>> = vec![None; tokens.len()];
    let mut last_sig: Option<usize> = None;
    for (i, t) in tokens.iter().enumerate() {
        prev_sig[i] = last_sig;
        if is_significant(&t.kind) {
            last_sig = Some(i);
        }
    }

    let mut next_sig: Vec<Option<usize>> = vec![None; tokens.len()];
    let mut after_sig: Option<usize> = None;
    for i in (0..tokens.len()).rev() {
        next_sig[i] = after_sig;
        if is_significant(&tokens[i].kind) {
            after_sig = Some(i);
        }
    }

    let mut i = 0;
    while i < tokens.len() {
        let t = &tokens[i];
        // Looking for: Dot, then significant Ident
        if matches!(t.kind, TokenKind::Dot) {
            if let Some(nxt_idx) = next_sig[i] {
                let nxt = &tokens[nxt_idx];
                if let TokenKind::Ident(name) = &nxt.kind {
                    let accessor = classify_accessor(name);
                    if let Some(target) = accessor {
                        // Check successor of nxt — must NOT be LParen (already a call).
                        if let Some(succ_idx) = next_sig[nxt_idx] {
                            let succ = &tokens[succ_idx];
                            if matches!(succ.kind, TokenKind::LParen) {
                                // Already `.len(...)` — skip.
                                i = nxt_idx + 1;
                                continue;
                            }
                        }
                        // Skip method-value form. Method-value pattern:
                        //   `let f = arr.len`   — RHS is a single chain
                        //   `let f fn() -> int = obj.field.len`
                        //
                        // Reliable detection: BOTH
                        //   (a) prev-of-chain == `=` (assignment context)
                        //   (b) next-after-Ident is **expression boundary**
                        //       (`Semicolon`, `Newline`, `RBrace`, `Comma`, `EOF`).
                        //
                        // Если (a) ИЛИ (b) не выполняется → это часть expression
                        // (`let x = a.len + 1`, `if a.len > 0`, `f(a.len, b)`),
                        // rewrite legitimate.
                        let chain_start = walk_chain_start(i, tokens, &prev_sig);
                        let prev_is_eq = prev_sig[chain_start]
                            .map(|p| matches!(tokens[p].kind, TokenKind::Eq))
                            .unwrap_or(false);
                        let after_ident = next_sig[nxt_idx];
                        let after_is_boundary = match after_ident {
                            None => true, // EOF
                            Some(s) => matches!(
                                tokens[s].kind,
                                TokenKind::Semicolon
                                    | TokenKind::RBrace
                                    | TokenKind::RParen
                                    | TokenKind::RBracket
                                    | TokenKind::Comma
                                    | TokenKind::Eof
                            ),
                        };
                        // Newline: Newline не is_significant'ит (skipped в prev_sig/next_sig),
                        // но семантически он — statement separator. Если between Ident и
                        // следующим significant нет non-Newline token'ов И между ними есть
                        // Newline, это конец statement.
                        let after_has_newline = (nxt_idx + 1..tokens.len())
                            .take_while(|&k| {
                                let kk = &tokens[k].kind;
                                matches!(kk, TokenKind::Newline)
                                    || (matches!(kk, TokenKind::Eof) && k == tokens.len() - 1)
                            })
                            .any(|k| matches!(tokens[k].kind, TokenKind::Newline));
                        let is_method_value = prev_is_eq && (after_is_boundary || after_has_newline);
                        if is_method_value {
                            i = nxt_idx + 1;
                            continue;
                        }

                        // Copy bytes before this rewrite (up to start of Ident).
                        out.push_str(&src[cursor..nxt.span.start]);
                        // Emit `target` (rewrites `cap` → `capacity`).
                        out.push_str(target);
                        // Skip original ident bytes (cursor moves past it).
                        cursor = nxt.span.end;
                        // Append `()`.
                        out.push_str("()");
                        changes += 1;
                        // Continue past this Ident.
                        i = nxt_idx + 1;
                        continue;
                    }
                }
            }
        }
        i += 1;
    }

    // Tail.
    out.push_str(&src[cursor..]);

    Ok(RewriteResult { text: out, changes })
}

/// Walk leftward от Dot-индекса `i` через chain'ы вида
/// `(@?Ident) (Dot Ident)*` и вернуть index самого левого token'а chain'а
/// (Ident или At). Использу­ется для skip-detection: если перед chain'ом
/// стоит `=`, это method-value.
///
/// Допустимые звенья влево от `Dot`:
///   - `Ident`        — поле / переменная
///   - `RBracket`     — конец `arr[i]` indexing, ищем дальше через `[ ... ]`
///   - `RParen`       — конец `f(args)`, аналогично
///   - `At` `Ident`   — `@field` self-access
///
/// Если что-то непредвиденное (например, оператор), возвращаем текущую
/// позицию — chain закончился.
fn walk_chain_start(i: usize, tokens: &[Token], prev_sig: &[Option<usize>]) -> usize {
    let mut k = i; // start at Dot
    loop {
        // Before Dot — must be Ident / RBracket / RParen (или конец `@field`).
        let Some(p) = prev_sig[k] else { return k };
        match &tokens[p].kind {
            TokenKind::Ident(_) => {
                // Check what's before this Ident:
                //   * `Dot`  — продолжаем влево
                //   * `At`   — `@field` self-access; останавливаемся на `At`
                //   * иначе  — chain закончился на этом Ident
                let Some(pp) = prev_sig[p] else { return p };
                match &tokens[pp].kind {
                    TokenKind::Dot => {
                        k = pp;
                        continue;
                    }
                    TokenKind::At => {
                        return pp;
                    }
                    _ => return p,
                }
            }
            TokenKind::RBracket => {
                // Skip backwards до matching `LBracket`.
                let lb = find_matching_open(tokens, p, TokenKind::LBracket, TokenKind::RBracket);
                if let Some(lb_idx) = lb {
                    // Before `[` must be Ident / RBracket / RParen.
                    let Some(before_lb) = prev_sig[lb_idx] else { return lb_idx };
                    k = before_lb_to_dot_start(before_lb, tokens, prev_sig);
                    if k == before_lb {
                        // Unchanged — chain ended.
                        return k;
                    }
                } else {
                    return p;
                }
            }
            TokenKind::RParen => {
                let lp = find_matching_open(tokens, p, TokenKind::LParen, TokenKind::RParen);
                if let Some(lp_idx) = lp {
                    let Some(before_lp) = prev_sig[lp_idx] else { return lp_idx };
                    k = before_lb_to_dot_start(before_lp, tokens, prev_sig);
                    if k == before_lp {
                        return k;
                    }
                } else {
                    return p;
                }
            }
            _ => return p,
        }
    }
}

/// Helper для walk-chain: после Index/Call, перед нами должен быть
/// Ident (receiver). Возвращаем точку для продолжения leftward chain.
fn before_lb_to_dot_start(p: usize, tokens: &[Token], prev_sig: &[Option<usize>]) -> usize {
    if matches!(tokens[p].kind, TokenKind::Ident(_)) {
        if let Some(pp) = prev_sig[p] {
            if matches!(tokens[pp].kind, TokenKind::Dot) {
                return pp;
            }
            if matches!(tokens[pp].kind, TokenKind::At) {
                return pp;
            }
        }
    }
    p
}

/// Brace-matching backwards: дано индекс `close` (RBracket/RParen), вернуть
/// matching open или None.
fn find_matching_open(
    tokens: &[Token],
    close_idx: usize,
    open: TokenKind,
    close: TokenKind,
) -> Option<usize> {
    let mut depth = 1i32;
    let mut k = close_idx;
    while k > 0 {
        k -= 1;
        if std::mem::discriminant(&tokens[k].kind) == std::mem::discriminant(&close) {
            depth += 1;
        } else if std::mem::discriminant(&tokens[k].kind) == std::mem::discriminant(&open) {
            depth -= 1;
            if depth == 0 {
                return Some(k);
            }
        }
    }
    None
}

/// Map accessor name → target name (with rename for `cap` → `capacity`).
/// Returns Some(target) iff this is a size-like accessor that should be
/// method-form per D117.
fn classify_accessor(name: &str) -> Option<&'static str> {
    match name {
        "len" => Some("len"),
        "is_empty" => Some("is_empty"),
        "byte_len" => Some("byte_len"),
        "cap" => Some("capacity"),
        "capacity" => Some("capacity"),
        _ => None,
    }
}

/// Markdown rewrite: extract ```nova / ```nv code blocks, rewrite each,
/// re-inject. Inline `\`code\`` ссылки tоо обрабатываются — simple
/// pattern: `\`arr.len\`` → `\`arr.len()\``.
fn rewrite_markdown(src: &str) -> Result<RewriteResult> {
    let mut out = String::with_capacity(src.len());
    let mut changes = 0usize;
    let mut i = 0;
    let bytes = src.as_bytes();

    while i < bytes.len() {
        // Detect fenced code block start: "```nova" or "```nv".
        if bytes[i..].starts_with(b"```") {
            // Find end of fence line.
            let fence_end = src[i..].find('\n').map(|j| i + j).unwrap_or(src.len());
            let header = &src[i..fence_end];
            let lang = header.trim_start_matches('`').trim();
            let is_nova = lang == "nova" || lang == "nv";
            // Copy header.
            out.push_str(&src[i..=fence_end.min(src.len() - 1)]);
            i = fence_end + 1;
            if i > src.len() {
                break;
            }
            // Find closing ``` on its own line (possibly with leading whitespace).
            let mut end = i;
            while end < bytes.len() {
                let line_end = src[end..].find('\n').map(|j| end + j).unwrap_or(src.len());
                let line = &src[end..line_end];
                if line.trim_start().starts_with("```") {
                    break;
                }
                end = line_end + 1;
            }
            let code = &src[i..end];
            if is_nova {
                let r = rewrite_nova(code)?;
                out.push_str(&r.text);
                changes += r.changes;
            } else {
                out.push_str(code);
            }
            i = end;
        } else if bytes[i] == b'`' {
            // Inline code ` ... `
            if let Some(rel_end) = src[i + 1..].find('`') {
                let inline_end = i + 1 + rel_end;
                let inner = &src[i + 1..inline_end];
                let rewritten = rewrite_inline_md_code(inner);
                if rewritten != inner {
                    changes += 1;
                }
                out.push('`');
                out.push_str(&rewritten);
                out.push('`');
                i = inline_end + 1;
            } else {
                out.push('`');
                i += 1;
            }
        } else {
            // Push char (UTF-8 aware: find next char boundary).
            let ch_end = (1..=4).find(|&j| src.is_char_boundary(i + j)).unwrap_or(1);
            out.push_str(&src[i..i + ch_end]);
            i += ch_end;
        }
    }

    Ok(RewriteResult { text: out, changes })
}

/// Inline `\`code\`` rewriter — simple regex-style replacement for
/// `\.<accessor>` patterns not followed by `(`. We don't lex inline
/// because it usually isn't valid Nova syntax (often just a snippet
/// fragment), but the same name rules apply.
fn rewrite_inline_md_code(s: &str) -> String {
    // Pattern: look for `.<accessor>` not followed by `(`.
    let accessors: &[&str] = &["len", "capacity", "byte_len", "is_empty", "cap"];
    let mut out = String::with_capacity(s.len() + 8);
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'.' && i + 1 < bytes.len() {
            // Find ident boundary after dot.
            let mut j = i + 1;
            while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                j += 1;
            }
            if j > i + 1 {
                let name = &s[i + 1..j];
                if accessors.contains(&name) {
                    // Not followed by '('?
                    if j == bytes.len() || bytes[j] != b'(' {
                        out.push('.');
                        out.push_str(if name == "cap" { "capacity" } else { name });
                        out.push_str("()");
                        i = j;
                        continue;
                    }
                }
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nova_roundtrip(src: &str) -> String {
        rewrite_nova(src).unwrap().text
    }

    #[test]
    fn basic_len_field_to_method_in_expression() {
        // `a.len + 1` — field-access используется in non-assignment expression.
        assert_eq!(
            nova_roundtrip("fn f(a []int) -> int { a.len + 1 }"),
            "fn f(a []int) -> int { a.len() + 1 }"
        );
    }

    #[test]
    fn basic_len_field_in_condition() {
        assert_eq!(
            nova_roundtrip("fn f(a []int) -> () { if a.len > 0 { } }"),
            "fn f(a []int) -> () { if a.len() > 0 { } }"
        );
    }

    #[test]
    fn basic_len_field_in_arg_position() {
        assert_eq!(
            nova_roundtrip("fn f(a []int) -> () { println(a.len) }"),
            "fn f(a []int) -> () { println(a.len()) }"
        );
    }

    #[test]
    fn already_method_unchanged() {
        let s = "fn f(a []int) -> int { a.len() }";
        assert_eq!(nova_roundtrip(s), s);
    }

    #[test]
    fn method_value_after_eq_preserved() {
        // `let f = arr.len` — bound method value, не делаем rewrite (D-block
        // Plan 11 method-values). Если user реально хочет method-call
        // в RHS of let — пишет `let n = arr.len()`.
        let s = "fn f(arr []int) -> () { let g = arr.len }";
        assert_eq!(nova_roundtrip(s), s);
    }

    #[test]
    fn cap_renamed_to_capacity_in_expression() {
        assert_eq!(
            nova_roundtrip("fn f(a []int) -> int { a.cap * 2 }"),
            "fn f(a []int) -> int { a.capacity() * 2 }"
        );
    }

    #[test]
    fn is_empty_in_condition() {
        assert_eq!(
            nova_roundtrip("fn f(a []int) -> () { if a.is_empty { } }"),
            "fn f(a []int) -> () { if a.is_empty() { } }"
        );
    }

    #[test]
    fn comments_preserved() {
        let s = "fn f(a []int /* size */) -> int { a.len + 1 }";
        assert_eq!(
            nova_roundtrip(s),
            "fn f(a []int /* size */) -> int { a.len() + 1 }"
        );
    }

    #[test]
    fn self_field_at_access() {
        // `@buckets.cap` — SelfAccess pattern; rewrite to `@buckets.capacity()`.
        assert_eq!(
            nova_roundtrip("fn T @method() -> int => @buckets.cap"),
            "fn T @method() -> int => @buckets.capacity()"
        );
    }

    #[test]
    fn nested_chain_field_access() {
        // `obj.field.len` — chain; len находится в конце, rewrite ок.
        assert_eq!(
            nova_roundtrip("fn f(o T) -> int { o.field.len + 0 }"),
            "fn f(o T) -> int { o.field.len() + 0 }"
        );
    }

    #[test]
    fn inline_md_code_rewrite() {
        assert_eq!(rewrite_inline_md_code("arr.len"), "arr.len()");
        assert_eq!(rewrite_inline_md_code("arr.cap"), "arr.capacity()");
        assert_eq!(rewrite_inline_md_code("arr.len()"), "arr.len()");
        assert_eq!(rewrite_inline_md_code("not a dot"), "not a dot");
    }

    #[test]
    fn unrelated_dots_unchanged() {
        let s = "fn main() -> () { let r = Range { start: 0, end: 5, inclusive: false }; let s = r.start }";
        assert_eq!(nova_roundtrip(s), s);
    }
}
