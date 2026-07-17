//! TEMPORARY (Plan 185 style-lints sweep, 2026-07-17): one-shot codemod to
//! bring `nova lint std spec_tests examples` to 0 under the two new rules
//! W_NON_COMPOUND_ASSIGN / W_WHILE_COUNTER_FOR_RANGE (precedent:
//! migrate_plan60/65 — same one-shot-tool pattern). DELETE this file + its
//! `[[bin]]` entry in Cargo.toml + the `pub fn find_*_edits` plumbing in
//! `compiler-codegen/src/lints.rs` once the sweep is committed.
//!
//! Two independent phases per file, in this order:
//!   1. W_WHILE_COUNTER_FOR_RANGE (structural) — iterative innermost-first
//!      rounds: nested counters (e.g. string_builder.nv's c/b loops) overlap
//!      in byte-range, so each round re-parses and selects only
//!      non-overlapping, smallest-span (innermost) candidates, applies them,
//!      and loops until none remain. Must run BEFORE phase 2 — the counter's
//!      own `i = i + 1` / `i += 1` increment is DELETED by this phase, so it
//!      must not also be visited by phase 2.
//!   2. W_NON_COMPOUND_ASSIGN — single pass, span-precise: `target = target
//!      OP right` spliced to `target OP= right` using the ORIGINAL source
//!      text at each operand's span (no reformatting risk).
//!
//! WHILE_COUNTER_SKIP: conformance fixtures whose own docstring says they
//! test the compiler's LICM/field-cache optimizer behavior specifically on
//! `while`-shaped loops (Plan 123 family), or isolate raw-loop perf overhead
//! — rewriting the loop shape risks silently exercising a different codegen
//! path than the one the test was written to pin (D58 for-in desugars
//! through the iterator protocol, a different lowering than a raw
//! comparison-based `while`). These get `nova:allow` by hand instead
//! (owner-reviewed exceptions, D428), not auto-rewritten.
//!
//! Modes: --dry-run (default, no writes) / --apply. Positional args = paths
//! (default: std spec_tests examples).

use anyhow::{Context, Result};
use nova_codegen::ast::Module;
use nova_codegen::lexer::lex;
use nova_codegen::lints::{find_non_compound_assign_edits, find_while_counter_edits};
use nova_codegen::parser::Parser;
use std::path::{Path, PathBuf};

/// Conformance fixtures deliberately EXCLUDED from the while-counter
/// auto-rewrite (see module doc) — `nova:allow`'d by hand instead.
const WHILE_COUNTER_SKIP: &[&str] = &[
    "chain_in_loop_ok.nv",
    "ipa_licm_mut_hoist_with_non_writing_call_ok.nv",
    "licm_escape_hatch_ok.nv",
    "licm_mut_after_call_ok.nv",
    "licm_nested_loops_ok.nv",
    "licm_ro_in_loop_ok.nv",
    "licm_zero_iter_safe_ok.nv",
    "loop_iteration_ok.nv",
    "m5_v2_1_licm_weighted_ok.nv",
    "neg_mut_write_in_loop_ok.nv",
    "neg_parallel_for_skip_ok.nv",
    "plan123_1_2_v1_2_nested_while_body_ok.nv",
    "plan123_2_1_v2_1_loop_body_weighted_ok.nv",
    "plan123_2_licm_mut_after_call_ok.nv",
    "plan123_2_licm_zero_iter_safe_ok.nv",
    "plan123_7_1_prop_ipa_semantic_equivalence_ok.nv",
    "plan123_7_2_v72_explicit_ipa_threading_ok.nv",
    "plan123_followups_2026_06_05_m5_v2_1_licm_weighted_ok.nv",
    "prop_ipa_semantic_equivalence_ok.nv",
    "prop_licm_composition_ok.nv",
    "prop_licm_semantic_equiv_ok.nv",
    "prop_pure_escape_hatch_ok.nv",
    "prop_threshold_invariance_ok.nv",
    "perf_contract_hot_loop_slow.nv",
    "v2_1_loop_body_weighted_ok.nv",
    "v72_explicit_ipa_threading_ok.nv",
];

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut apply = false;
    let mut paths: Vec<PathBuf> = Vec::new();
    for a in &args {
        match a.as_str() {
            "--apply" => apply = true,
            "--dry-run" => apply = false,
            other => paths.push(PathBuf::from(other)),
        }
    }
    if paths.is_empty() {
        paths = vec!["std".into(), "spec_tests".into(), "examples".into()];
    }

    let mut files = Vec::new();
    for p in &paths {
        walk(p, &mut files)?;
    }
    files.sort();

    let mut total_compound = 0usize;
    let mut total_while = 0usize;
    let mut touched = 0usize;
    let mut parse_failures = 0usize;

    for f in &files {
        let src0 =
            std::fs::read_to_string(f).with_context(|| format!("read {}", f.display()))?;
        let file_name = f.file_name().and_then(|s| s.to_str()).unwrap_or("");
        let skip_while = WHILE_COUNTER_SKIP.contains(&file_name);

        let mut src = src0.clone();
        let mut while_changes = 0usize;
        if !skip_while {
            for _round in 0..8 {
                let Some(m) = parse_quiet(&src) else { break };
                let mut edits = find_while_counter_edits(&m);
                if edits.is_empty() {
                    break;
                }
                // Innermost (smallest span) first.
                edits.sort_by_key(|e| e.whole_end - e.whole_start);
                let mut selected: Vec<usize> = Vec::new();
                'outer: for idx in 0..edits.len() {
                    for &si in &selected {
                        let (a, b) = (&edits[idx], &edits[si]);
                        if a.whole_start < b.whole_end && b.whole_start < a.whole_end {
                            continue 'outer;
                        }
                    }
                    selected.push(idx);
                }
                if selected.is_empty() {
                    break;
                }
                selected.sort_by_key(|&i| std::cmp::Reverse(edits[i].whole_start));
                for i in selected {
                    let e = &edits[i];
                    let start_src = &src[e.start_start..e.start_end];
                    let end_src = &src[e.end_start..e.end_end];
                    let body_new = splice_body_without_last(
                        &src,
                        e.body_start,
                        e.body_end,
                        e.last_stmt_start,
                        e.last_stmt_end,
                    );
                    let replacement = format!(
                        "for {} in {}{}{} {}",
                        e.name, start_src, e.range_op, end_src, body_new
                    );
                    src.replace_range(e.whole_start..e.whole_end, &replacement);
                    while_changes += 1;
                }
            }
        }

        let mut compound_changes = 0usize;
        if let Some(m) = parse_quiet(&src) {
            let mut edits = find_non_compound_assign_edits(&m);
            edits.sort_by_key(|e| std::cmp::Reverse(e.whole_start));
            for e in &edits {
                let target_src = src[e.target_start..e.target_end].to_string();
                let right_src = src[e.right_start..e.right_end].to_string();
                let replacement = format!("{} {} {}", target_src, e.compound, right_src);
                src.replace_range(e.whole_start..e.whole_end, &replacement);
                compound_changes += 1;
            }
        }

        if src != src0 {
            // Safety: never write a file whose transformed text fails to
            // parse — surface it instead (leaves the original untouched).
            if parse_quiet(&src).is_none() {
                parse_failures += 1;
                eprintln!("PARSE FAILED after rewrite, skipping write: {}", f.display());
                continue;
            }
            touched += 1;
            total_while += while_changes;
            total_compound += compound_changes;
            println!(
                "{}: {} while-counter, {} compound-assign",
                f.display(),
                while_changes,
                compound_changes
            );
            if apply {
                std::fs::write(f, &src).with_context(|| format!("write {}", f.display()))?;
            }
        }
    }

    println!();
    println!("=== Summary ===");
    println!("Files scanned    : {}", files.len());
    println!("Files changed    : {}", touched);
    println!("while-counter    : {}", total_while);
    println!("compound-assign  : {}", total_compound);
    println!("parse failures   : {}", parse_failures);
    if !apply {
        println!("(dry-run — use --apply to write)");
    }
    if parse_failures > 0 {
        std::process::exit(1);
    }
    Ok(())
}

fn parse_quiet(src: &str) -> Option<Module> {
    let toks = lex(src).ok()?;
    let mut p = Parser::new(toks);
    p.parse_module().ok()
}

/// Original body text (`{ ... }`, byte range `[body_start, body_end)`) with
/// the last statement (`[last_start, last_end)`) removed: keep everything up
/// to it (trimmed of trailing whitespace and at most one separator `;`) plus
/// everything from right after it through the closing `}` (kept AS-IS —
/// preserves original indentation of the closing brace for multi-line bodies).
fn splice_body_without_last(
    src: &str,
    body_start: usize,
    body_end: usize,
    last_start: usize,
    last_end: usize,
) -> String {
    let prefix = &src[body_start..last_start];
    let prefix_trimmed = prefix.trim_end_matches(|c: char| c.is_whitespace());
    let prefix_trimmed = prefix_trimmed.strip_suffix(';').unwrap_or(prefix_trimmed);
    let suffix = &src[last_end..body_end];
    format!("{prefix_trimmed}{suffix}")
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    if dir.is_file() {
        if dir.extension().and_then(|s| s.to_str()) == Some("nv") {
            out.push(dir.to_path_buf());
        }
        return Ok(());
    }
    for entry in std::fs::read_dir(dir).with_context(|| format!("read_dir {}", dir.display()))? {
        let entry = entry?;
        let p = entry.path();
        if p.is_dir() {
            let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if name == "target" || name == ".git" || name == "node_modules" {
                continue;
            }
            walk(&p, out)?;
        } else if p.extension().and_then(|s| s.to_str()) == Some("nv") {
            out.push(p);
        }
    }
    Ok(())
}
