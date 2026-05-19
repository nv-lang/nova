//! Plan 65 Ф.4: lexer-based migration tool — `Time.after(<arg>)` →
//! `ChanReader.close_after(Duration.from_*(<arg>))`.
//!
//! Rewrite rules (Plan 65 AD11):
//!   `Time.after(<INT_LIT>)`           → `ChanReader.close_after(Duration.from_millis(<INT_LIT>))`
//!   `Time.after(<FLOAT_LIT>)`         → `ChanReader.close_after(Duration.from_secs_f64(<FLOAT_LIT>))`
//!   `Time.after(<other-expression>)`  → leave as-is + emit comment
//!                                       `// MIGRATE_MANUAL: Plan 65 — non-literal arg`
//!                                       (CI dry-run gate exits 1 on this).
//!
//! Skip conditions (token-aware via nova_codegen lexer):
//!   - `Time.after` inside string literal — lexer never tokenises it
//!     as Ident(Time)/Dot/Ident(after), so naturally skipped.
//!   - `Time.after` inside `//` or `///` comment — lexer emits Comment
//!     token; we only match Ident chains so naturally skipped.
//!
//! Markdown rewriter walks ```nova / ```nv code blocks plus inline
//! `\`Time.after(...)\`` spans.
//!
//! Modes:
//!   --dry-run         только показать diff, не писать (default).
//!   --apply           реально записать.
//!   --md              включить .md файлы.
//!   --paths <dirs>    список директорий (default: std/ nova_tests/ examples/).
//!
//! Exit codes:
//!   0 — no changes needed (idempotent).
//!   1 — manual markers emitted (MIGRATE_MANUAL: ...) — CI gate fails.
//!   2 — changes applied (or would be applied in dry-run).

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
                eprintln!("usage: migrate_plan65 [--apply] [--dry-run] [--md] [--paths DIR ...]");
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
    let mut total_manual = 0usize;
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
                total_manual += rewritten.manual_markers;
                println!(
                    "{}: {} change(s){}",
                    f.display(),
                    rewritten.changes,
                    if rewritten.manual_markers > 0 {
                        format!(", {} MIGRATE_MANUAL marker(s)", rewritten.manual_markers)
                    } else {
                        String::new()
                    }
                );
                if opts.apply {
                    std::fs::write(&f, &rewritten.text)
                        .with_context(|| format!("write {}", f.display()))?;
                }
            }
        }
    }

    println!();
    println!("=== Summary ===");
    println!("Files scanned        : {}", total_files);
    println!("Files changed        : {}", touched_files);
    println!("Total rewrites       : {}", total_rewrites);
    println!("MIGRATE_MANUAL marks : {}", total_manual);
    if !opts.apply {
        println!("(dry-run — use --apply to actually write)");
    }

    // Exit codes:
    //   1 — manual markers present (CI gate fails).
    //   2 — changes performed (apply mode) or would be performed (dry-run).
    //   0 — nothing to do (idempotent).
    if total_manual > 0 {
        std::process::exit(1);
    }
    if total_rewrites > 0 {
        std::process::exit(2);
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
    manual_markers: usize,
}

/// Token-level rewrite for Nova-source.
///
/// Strategy: find `Ident("Time") Dot Ident("after") LParen <arg-tokens> RParen`
/// patterns. The arg-tokens are scanned to detect:
///   * exactly one IntLit  → wrap with Duration.from_millis(N)
///   * exactly one FloatLit → wrap with Duration.from_secs_f64(N)
///   * anything else        → emit MIGRATE_MANUAL comment, leave call.
fn rewrite_nova(src: &str) -> Result<RewriteResult> {
    let tokens = match lex(src) {
        Ok(t) => t,
        Err(_) => {
            // Lex errors: leave file unchanged, do not pretend to migrate.
            return Ok(RewriteResult {
                text: src.to_string(),
                changes: 0,
                manual_markers: 0,
            });
        }
    };
    rewrite_tokens(src, &tokens)
}

fn rewrite_tokens(src: &str, tokens: &[Token]) -> Result<RewriteResult> {
    let mut out = String::with_capacity(src.len() + 128);
    let mut cursor: usize = 0;
    let mut changes = 0usize;
    let mut manual_markers = 0usize;

    // Track whether file already imports std.time.duration; if not, we will
    // inject the import once we've finished rewriting (and only if at least
    // one rewrite occurred — manual markers don't require Duration).
    let needs_duration_import = !src.contains("import std.time.duration");

    let is_significant = |k: &TokenKind| !matches!(k, TokenKind::Newline | TokenKind::Eof);

    // Pre-compute next significant index lookup.
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
        if let TokenKind::Ident(name) = &t.kind {
            if name == "Time" {
                // Expect: Time . after ( ... )
                if let Some(dot_idx) = next_sig[i] {
                    if matches!(tokens[dot_idx].kind, TokenKind::Dot) {
                        if let Some(meth_idx) = next_sig[dot_idx] {
                            if let TokenKind::Ident(m) = &tokens[meth_idx].kind {
                                if m == "after" {
                                    if let Some(lp_idx) = next_sig[meth_idx] {
                                        if matches!(tokens[lp_idx].kind, TokenKind::LParen) {
                                            // Match found. Locate matching RParen.
                                            if let Some(rp_idx) = find_matching_close(
                                                tokens,
                                                lp_idx,
                                                TokenKind::LParen,
                                                TokenKind::RParen,
                                            ) {
                                                // Extract significant arg tokens (between
                                                // lp_idx+1..rp_idx exclusive), skipping
                                                // newlines.
                                                let arg_tokens: Vec<&Token> =
                                                    tokens[lp_idx + 1..rp_idx]
                                                        .iter()
                                                        .filter(|t| is_significant(&t.kind))
                                                        .collect();
                                                // Classify the argument. Use original
                                                // source text via spans so 10_000-style
                                                // literals are preserved.
                                                let classification = classify_arg(&arg_tokens, src);
                                                let call_start = t.span.start;
                                                let call_end = tokens[rp_idx].span.end;
                                                // Copy everything before this call.
                                                out.push_str(&src[cursor..call_start]);
                                                match classification {
                                                    ArgKind::Int(lit_text) => {
                                                        out.push_str(&format!(
                                                            "ChanReader.close_after(Duration.from_millis({}))",
                                                            lit_text
                                                        ));
                                                        changes += 1;
                                                    }
                                                    ArgKind::Float(lit_text) => {
                                                        out.push_str(&format!(
                                                            "ChanReader.close_after(Duration.from_secs_f64({}))",
                                                            lit_text
                                                        ));
                                                        changes += 1;
                                                    }
                                                    ArgKind::Other => {
                                                        // Preserve the original call text
                                                        // and prepend a MIGRATE_MANUAL marker
                                                        // comment on the same line.
                                                        out.push_str(&format!(
                                                            "/* MIGRATE_MANUAL: Plan 65 — non-literal Time.after arg; \
                                                             rewrite manually to ChanReader.close_after(Duration.<...>) */ {}",
                                                            &src[call_start..call_end]
                                                        ));
                                                        manual_markers += 1;
                                                    }
                                                }
                                                cursor = call_end;
                                                i = rp_idx + 1;
                                                continue;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        i += 1;
    }

    out.push_str(&src[cursor..]);

    // Inject `import std.time.duration` after the `module ...` line, but
    // only when at least one rewrite happened — manual-marker-only files
    // already have their original Time.after call so they still need a
    // Duration in scope after the user finishes migrating manually. So
    // also include manual markers in the gate.
    if (changes > 0 || manual_markers > 0) && needs_duration_import {
        out = inject_duration_import(&out);
    }

    Ok(RewriteResult { text: out, changes, manual_markers })
}

/// Insert `import std.time.duration` right after the `module ...` line.
/// Idempotent: if the import is already present we return the input.
fn inject_duration_import(src: &str) -> String {
    if src.contains("import std.time.duration") {
        return src.to_string();
    }
    let mut out = String::with_capacity(src.len() + 32);
    let mut injected = false;
    for line in src.lines() {
        out.push_str(line);
        out.push('\n');
        if !injected && line.trim_start().starts_with("module ") {
            out.push('\n');
            out.push_str("import std.time.duration\n");
            injected = true;
        }
    }
    if src.ends_with('\n') {
        out
    } else {
        // strip trailing newline we added
        out.trim_end_matches('\n').to_string()
    }
}

enum ArgKind {
    Int(String),
    Float(String),
    Other,
}

/// Classify the argument tokens of a `Time.after(<args>)` call.
///
/// Accepts:
///   * exactly one Int(_) (possibly preceded by Minus for negative)
///   * exactly one Float(_) (possibly preceded by Minus)
/// Everything else → Other (forces manual review).
///
/// The original source spans are used to preserve underscored numeric
/// literals (`10_000` etc.).
fn classify_arg(args: &[&Token], src: &str) -> ArgKind {
    if args.is_empty() {
        return ArgKind::Other;
    }
    // Handle optional leading unary minus.
    let (sign, rest) = match &args[0].kind {
        TokenKind::Minus => ("-", &args[1..]),
        _ => ("", &args[..]),
    };
    if rest.len() != 1 {
        return ArgKind::Other;
    }
    let tok = rest[0];
    let lit_text = &src[tok.span.start..tok.span.end];
    match &tok.kind {
        TokenKind::Int(_) => ArgKind::Int(format!("{}{}", sign, lit_text)),
        TokenKind::Float(_) => ArgKind::Float(format!("{}{}", sign, lit_text)),
        _ => ArgKind::Other,
    }
}

/// Brace-matching forwards.
fn find_matching_close(
    tokens: &[Token],
    open_idx: usize,
    open: TokenKind,
    close: TokenKind,
) -> Option<usize> {
    let mut depth = 1i32;
    let mut k = open_idx;
    while k + 1 < tokens.len() {
        k += 1;
        if std::mem::discriminant(&tokens[k].kind) == std::mem::discriminant(&open) {
            depth += 1;
        } else if std::mem::discriminant(&tokens[k].kind) == std::mem::discriminant(&close) {
            depth -= 1;
            if depth == 0 {
                return Some(k);
            }
        }
    }
    None
}

/// Markdown rewrite: walk ```nova / ```nv code fences, rewrite each.
/// Inline `\`code\`` snippets also processed (best-effort regex-style).
fn rewrite_markdown(src: &str) -> Result<RewriteResult> {
    let mut out = String::with_capacity(src.len());
    let mut changes = 0usize;
    let mut manual_markers = 0usize;
    let mut i = 0;
    let bytes = src.as_bytes();

    while i < bytes.len() {
        if bytes[i..].starts_with(b"```") {
            let fence_end = src[i..].find('\n').map(|j| i + j).unwrap_or(src.len());
            let header = &src[i..fence_end];
            let lang = header.trim_start_matches('`').trim();
            let is_nova = lang == "nova" || lang == "nv";
            out.push_str(&src[i..=fence_end.min(src.len() - 1)]);
            i = fence_end + 1;
            if i > src.len() {
                break;
            }
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
                manual_markers += r.manual_markers;
            } else {
                out.push_str(code);
            }
            i = end;
        } else if bytes[i] == b'`' {
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
            let ch_end = (1..=4).find(|&j| src.is_char_boundary(i + j)).unwrap_or(1);
            out.push_str(&src[i..i + ch_end]);
            i += ch_end;
        }
    }

    Ok(RewriteResult { text: out, changes, manual_markers })
}

/// Inline-code rewrite — simple regex-style: `Time.after(<lit>)` → rewritten.
fn rewrite_inline_md_code(s: &str) -> String {
    // Quick check.
    if !s.contains("Time.after(") {
        return s.to_string();
    }
    // We delegate to the full token-aware rewriter on the inline string
    // — it accepts any Nova snippet as long as lex succeeds.
    match rewrite_nova(s) {
        Ok(r) => r.text,
        Err(_) => s.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrites_int_literal() {
        let src = "test \"t\" { let _ = Time.after(50) }\n";
        let r = rewrite_nova(src).unwrap();
        assert!(r.text.contains("ChanReader.close_after(Duration.from_millis(50))"));
        assert_eq!(r.changes, 1);
        assert_eq!(r.manual_markers, 0);
    }

    #[test]
    fn rewrites_float_literal() {
        let src = "let _ = Time.after(2.5)\n";
        let r = rewrite_nova(src).unwrap();
        assert!(r.text.contains("ChanReader.close_after(Duration.from_secs_f64(2.5))"));
    }

    #[test]
    fn manual_marker_for_non_literal() {
        let src = "let ms = 50\nlet _ = Time.after(ms)\n";
        let r = rewrite_nova(src).unwrap();
        assert!(r.text.contains("MIGRATE_MANUAL"));
        assert_eq!(r.manual_markers, 1);
        assert_eq!(r.changes, 0);
    }

    #[test]
    fn skips_strings_and_comments() {
        let src = r#"// Time.after(50) in comment
let s = "Time.after(50)"  // string literal
"#;
        let r = rewrite_nova(src).unwrap();
        assert_eq!(r.changes, 0);
        assert_eq!(r.manual_markers, 0);
        assert_eq!(r.text, src);
    }

    #[test]
    fn idempotent_on_already_migrated() {
        let src = "let _ = ChanReader.close_after(Duration.from_millis(50))\n";
        let r1 = rewrite_nova(src).unwrap();
        assert_eq!(r1.text, src);
        assert_eq!(r1.changes, 0);
    }

    #[test]
    fn negative_int_literal() {
        // Negative duration eventually panics at runtime; tool still rewrites
        // mechanically so user can verify intent.
        let src = "let _ = Time.after(-1)\n";
        let r = rewrite_nova(src).unwrap();
        assert!(r.text.contains("Duration.from_millis(-1)"));
    }

    #[test]
    fn underscore_int_literal() {
        let src = "let _ = Time.after(10_000)\n";
        let r = rewrite_nova(src).unwrap();
        assert!(r.text.contains("Duration.from_millis(10_000)"));
    }

    #[test]
    fn injects_duration_import_after_module() {
        let src = "module foo.bar\n\ntest \"t\" { let _ = Time.after(50) }\n";
        let r = rewrite_nova(src).unwrap();
        assert!(r.text.contains("import std.time.duration"));
        // Import must come after `module ...`.
        let mod_pos = r.text.find("module foo.bar").unwrap();
        let imp_pos = r.text.find("import std.time.duration").unwrap();
        assert!(imp_pos > mod_pos);
    }

    #[test]
    fn does_not_double_import_duration() {
        let src = "module foo.bar\n\nimport std.time.duration\n\ntest \"t\" { let _ = Time.after(50) }\n";
        let r = rewrite_nova(src).unwrap();
        assert_eq!(r.text.matches("import std.time.duration").count(), 1);
    }

    #[test]
    fn does_not_inject_import_when_no_rewrites() {
        let src = "module foo.bar\n\ntest \"t\" { assert(true) }\n";
        let r = rewrite_nova(src).unwrap();
        assert!(!r.text.contains("import std.time.duration"));
    }
}
