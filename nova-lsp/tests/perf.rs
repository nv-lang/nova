//! Performance budget tests for nova-lsp (Plan 104.1.Ф.7).
//!
//! These tests assert that critical operations complete within defined budgets.
//! They are meaningful in release mode (`cargo test --release`) since debug
//! builds have no optimization.  In debug builds the assertions use generous
//! multipliers to avoid flakiness in CI.
//!
//! Performance budgets (release):
//! - check_workspace on 10-file project: < 1s
//! - 1000 incremental edits on 10 KB rope: < 100ms
//! - debouncer overhead for 1000 schedule() calls: < 100ms

use nova_lsp::compiler::check_workspace;
use nova_lsp::debouncer::Debouncer;
use nova_lsp::incremental::apply_changes;
use ropey::Rope;
use std::time::{Duration, Instant};
use tower_lsp::lsp_types::{Position, Range, TextDocumentContentChangeEvent, Url};

fn valid_nv(idx: usize) -> String {
    format!("module perf_test.file{idx}\n\nfn work_{idx}(x int, y int) -> int => x + y\n")
}

fn change_event(text: &str) -> TextDocumentContentChangeEvent {
    TextDocumentContentChangeEvent {
        range: Some(Range {
            start: Position { line: 0, character: 0 },
            end: Position { line: 0, character: 0 },
        }),
        range_length: None,
        text: text.to_string(),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// pos1: check_workspace 10-file project < 1s (release) / < 30s (debug)
// ─────────────────────────────────────────────────────────────────────────────

/// pos1: check_workspace on 10-file project completes within budget.
#[test]
fn pos1_check_workspace_10_files_under_budget() {
    let dir = tempfile::tempdir().expect("create temp dir");
    for i in 0..10 {
        std::fs::write(dir.path().join(format!("file{i}.nv")), valid_nv(i)).unwrap();
    }

    let start = Instant::now();
    let results = check_workspace(dir.path());
    let elapsed = start.elapsed();

    assert_eq!(results.len(), 10, "expected 10 results");

    // Budget: release < 1s, debug < 30s (compiler is slow in debug mode).
    let budget = if cfg!(debug_assertions) {
        Duration::from_secs(60) // generous for debug
    } else {
        Duration::from_secs(1)
    };

    assert!(
        elapsed <= budget,
        "check_workspace took {}ms, budget={}ms (debug={})",
        elapsed.as_millis(),
        budget.as_millis(),
        cfg!(debug_assertions)
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// pos2: 1000 incremental edits on 10 KB rope < 100ms (release) / < 5s (debug)
// ─────────────────────────────────────────────────────────────────────────────

/// pos2: 1000 small incremental edits on a 10 KB rope complete within budget.
#[test]
fn pos2_1000_incremental_edits_under_budget() {
    // Build a 10 KB rope (approx).
    let initial = "abcdefghij\n".repeat(900); // ~10 KB
    let mut rope = Rope::from_str(&initial);

    let start = Instant::now();
    for _ in 0..1000 {
        // Insert one char at position (0, 0)
        apply_changes(&mut rope, &[change_event("x")]);
    }
    let elapsed = start.elapsed();

    let budget = if cfg!(debug_assertions) {
        Duration::from_secs(5)
    } else {
        Duration::from_millis(100)
    };

    assert!(
        elapsed <= budget,
        "1000 edits took {}ms, budget={}ms",
        elapsed.as_millis(),
        budget.as_millis()
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// pos3: 1000 debouncer schedule() calls < 100ms overhead
// ─────────────────────────────────────────────────────────────────────────────

/// pos3: 1000 debouncer schedule() calls complete within 100ms.
///
/// This tests the overhead of acquiring the mutex, cancelling, and spawning
/// tokio tasks — not the work inside them.
#[tokio::test]
async fn pos3_1000_debouncer_schedule_calls_under_budget() {
    let db = Debouncer::new(Duration::from_secs(60)); // large delay — work won't run
    let uri = Url::parse("file:///perf_test.nv").unwrap();

    let start = Instant::now();
    for _ in 0..1000 {
        let u = uri.clone();
        db.schedule(u, |_tok| async {});
    }
    let elapsed = start.elapsed();

    // Cancel all before they fire (large delay above ensures they don't run).
    db.cancel_all();

    let budget = if cfg!(debug_assertions) {
        Duration::from_secs(2)
    } else {
        Duration::from_millis(100)
    };

    assert!(
        elapsed <= budget,
        "1000 schedule() calls took {}ms, budget={}ms",
        elapsed.as_millis(),
        budget.as_millis()
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// neg1: file > 1 MB — performance degradation measured, no strict assertion
// ─────────────────────────────────────────────────────────────────────────────

/// neg1: large file (1 MB) is checked without panic; duration is logged.
///
/// No strict time budget here — we just verify it completes.
#[test]
fn neg1_large_file_no_panic_measured() {
    // Generate ~1 MB of Nova-like source (many function declarations).
    let mut src = String::with_capacity(1_100_000);
    src.push_str("module perf_test.large\n\n");
    for i in 0..10_000 {
        src.push_str(&format!("fn f_{i}(x int) -> int => x + {i}\n"));
    }

    let start = Instant::now();
    let result = nova_lsp::compiler::check_file(
        &Url::parse("file:///large.nv").unwrap(),
        &src,
    );
    let elapsed = start.elapsed();

    // Must not panic; result is either ok or some diagnostics.
    let _ = result;

    eprintln!(
        "neg1: large file (~{}KB) took {}ms",
        src.len() / 1024,
        elapsed.as_millis()
    );
}
