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

/// Writes `n` synthetic `.nv` files into a fresh temp dir and returns the
/// minimum-of-3 wall time (ms) for `check_workspace` over it.
fn measure_check_workspace_ms(n: usize) -> (f64, usize) {
    let dir = tempfile::tempdir().expect("create temp dir");
    for i in 0..n {
        std::fs::write(dir.path().join(format!("file{i}.nv")), valid_nv(i)).unwrap();
    }
    let mut best = u128::MAX;
    let mut n_results = 0;
    for _ in 0..3 {
        let start = Instant::now();
        let results = check_workspace(dir.path());
        best = best.min(start.elapsed().as_millis());
        n_results = results.len();
    }
    (best as f64, n_results)
}

/// pos1: check_workspace cost scales roughly linearly with file count.
///
/// Registry #1159/#1154 (2026-09-30): this used to assert an ABSOLUTE
/// wall-clock budget (release<1s / debug<30s), which judges the MACHINE
/// running the test (red under gate load, green alone), not check_workspace
/// itself. Fix: measure at two file counts BACK-TO-BACK IN THE SAME RUN and
/// assert the cost RATIO stays close to the file-count ratio (5x); load
/// inflates both arms together so the ratio holds even when the raw
/// milliseconds do not, and an O(n^2) regression still reddens.
#[test]
fn pos1_check_workspace_10_files_under_budget() {
    let (small_ms, small_n) = measure_check_workspace_ms(2);
    let (big_ms, big_n) = measure_check_workspace_ms(10);

    assert_eq!(small_n, 2, "expected 2 results");
    assert_eq!(big_n, 10, "expected 10 results");

    let ratio = big_ms / small_ms.max(1.0);
    assert!(
        ratio < 15.0,
        "check_workspace cost grew {:.1}x for a 5x larger workspace (2-file={} ms, \
         10-file={} ms) — expected near-linear scaling (~5x); this looks like an \
         O(n^2) regression, not machine load (registry #1159)",
        ratio, small_ms, big_ms
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// pos2: 1000 incremental edits on 10 KB rope < 100ms (release) / < 5s (debug)
// ─────────────────────────────────────────────────────────────────────────────

/// Applies `n` small incremental edits to a fresh 10 KB rope and returns the
/// wall time (ms).
fn measure_incremental_edits_ms(n: usize) -> f64 {
    let initial = "abcdefghij\n".repeat(900); // ~10 KB
    let mut rope = Rope::from_str(&initial);
    let start = Instant::now();
    for _ in 0..n {
        // Insert one char at position (0, 0)
        apply_changes(&mut rope, &[change_event("x")]);
    }
    start.elapsed().as_millis() as f64
}

/// pos2: 1000 small incremental edits on a 10 KB rope scale roughly linearly
/// with edit count.
///
/// Registry #1159/#1154 (2026-09-30): this used to assert an ABSOLUTE
/// wall-clock budget (release<100ms / debug<5s), which judges the MACHINE,
/// not `apply_changes`. Fix: measure at two edit counts BACK-TO-BACK IN THE
/// SAME RUN and assert the cost RATIO stays close to the count ratio (5x) —
/// see pos1's doc comment above for the full rationale.
#[test]
fn pos2_1000_incremental_edits_under_budget() {
    let small_ms = measure_incremental_edits_ms(200).max(1.0);
    let big_ms = measure_incremental_edits_ms(1000);

    let ratio = big_ms / small_ms;
    assert!(
        ratio < 15.0,
        "1000 edits cost grew {:.1}x vs 200 edits (5x more work) (200={} ms, \
         1000={} ms) — expected near-linear scaling (~5x); this looks like an \
         O(n^2) regression in apply_changes, not machine load (registry #1159)",
        ratio, small_ms, big_ms
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// pos3: 1000 debouncer schedule() calls < 100ms overhead
// ─────────────────────────────────────────────────────────────────────────────

/// Issues `n` debouncer `schedule()` calls (then cancels them all before
/// they fire) and returns the wall time (ms) for the scheduling loop alone.
async fn measure_debouncer_schedule_ms(n: usize) -> f64 {
    let db = Debouncer::new(Duration::from_secs(60)); // large delay — work won't run
    let uri = Url::parse("file:///perf_test.nv").unwrap();

    let start = Instant::now();
    for _ in 0..n {
        let u = uri.clone();
        db.schedule(u, |_tok| async {});
    }
    let elapsed_ms = start.elapsed().as_millis() as f64;

    // Cancel all before they fire (large delay above ensures they don't run).
    db.cancel_all();
    elapsed_ms
}

/// pos3: debouncer schedule() overhead (mutex + cancel + tokio spawn — not
/// the work inside them) scales roughly linearly with call count.
///
/// Registry #1159/#1154 (2026-09-30): this used to assert an ABSOLUTE
/// wall-clock budget (release<100ms / debug<2s), which judges the MACHINE,
/// not `schedule()`. Fix: measure at two call counts BACK-TO-BACK IN THE
/// SAME RUN and assert the cost RATIO stays close to the count ratio (5x) —
/// see pos1's doc comment above for the full rationale.
#[tokio::test]
async fn pos3_1000_debouncer_schedule_calls_under_budget() {
    let small_ms = measure_debouncer_schedule_ms(200).await.max(1.0);
    let big_ms = measure_debouncer_schedule_ms(1000).await;

    let ratio = big_ms / small_ms;
    assert!(
        ratio < 15.0,
        "1000 schedule() calls cost grew {:.1}x vs 200 calls (5x more work) \
         (200={} ms, 1000={} ms) — expected near-linear scaling (~5x); this \
         looks like an O(n^2) regression, not machine load (registry #1159)",
        ratio, small_ms, big_ms
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Plan 213 Ф.1: open-documents recheck vs full-workspace recheck
// ─────────────────────────────────────────────────────────────────────────────

/// Plan 213 Ф.1 regression guard: `check_open_documents` (the new per-edit
/// recheck strategy used by `schedule_recheck`) must be dramatically cheaper
/// than `check_workspace` (the *old* per-edit strategy — every `.nv` file
/// under the workspace root, re-parsed + import-resolved + type-checked, on
/// every debounced edit) on a real, large workspace. That gap is the whole
/// point of the fix (diagnosed: nova-lsp was burning ~27 CPU-hours/day
/// because every keystroke triggered a `check_workspace` over the entire
/// Nova repo — 3000+ files across std/examples/spec_tests/nova_tests).
///
/// `#[ignore]`d because it runs against the *actual* repo checkout (not a
/// synthetic fixture) so it is unsuitable for a fast default `cargo test`
/// run; execute manually with:
/// `cargo test --release --test perf -- --ignored --nocapture`
#[test]
#[ignore]
fn check_open_documents_much_cheaper_than_check_workspace_on_real_repo() {
    use nova_lsp::compiler::check_open_documents;

    let repo_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("nova-lsp has a parent")
        .to_path_buf();

    let t0 = Instant::now();
    let workspace_results = check_workspace(&repo_root);
    let workspace_elapsed = t0.elapsed();
    eprintln!(
        "check_workspace(real repo, {} files): {}ms",
        workspace_results.len(),
        workspace_elapsed.as_millis()
    );

    // Simulate a realistic edit session: 2 open documents, resolved against
    // the same workspace root (exactly what `schedule_recheck_for` now does
    // on every debounced edit).
    let docs: Vec<(Url, String)> = (0..2)
        .map(|i| {
            let uri = Url::parse(&format!("file:///open_doc_{i}.nv")).unwrap();
            (uri, valid_nv(i))
        })
        .collect();

    let t1 = Instant::now();
    let open_results = check_open_documents(&docs, &repo_root);
    let open_elapsed = t1.elapsed();
    eprintln!(
        "check_open_documents(2 open docs, same repo): {}ms",
        open_elapsed.as_millis()
    );

    assert_eq!(open_results.len(), 2);
    assert!(
        workspace_results.len() > 100,
        "sanity: the real repo must have >100 .nv files for this comparison to \
         be meaningful (found {}) — run from within the Nova repo checkout",
        workspace_results.len()
    );
    assert!(
        open_elapsed.saturating_mul(3) < workspace_elapsed,
        "open-documents recheck ({}ms) must be dramatically (>=3x) cheaper than \
         a full workspace recheck ({}ms) over {} files — this ratio IS the Plan \
         213 Ф.1 fix; a regression here means schedule_recheck is back to \
         O(workspace size) per edit",
        open_elapsed.as_millis(),
        workspace_elapsed.as_millis(),
        workspace_results.len(),
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
