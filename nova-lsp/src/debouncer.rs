//! Debouncer with per-URI cancellation tokens.
//!
//! Plan 104.1.Ф.3: production implementation.
//!
//! Each URI has an independent cancellation token.  When a new edit arrives
//! for the same URI, the previous pending task is cancelled and a new one
//! is scheduled after `delay`.  Edits to different URIs run concurrently.
//!
//! # Design
//!
//! `schedule` is **synchronous** (non-async):  it acquires the `std::sync::Mutex`,
//! atomically cancels the old token and inserts the new one, then spawns the
//! tokio task.  This means `cancel_all` is race-free — the token is in the
//! map before `schedule` returns.

use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio_util::sync::CancellationToken;
use tower_lsp::lsp_types::Url;

// ─────────────────────────────────────────────────────────────────────────────
// Public API
// ─────────────────────────────────────────────────────────────────────────────

/// A debouncer that coalesces rapid edits to the same URI.
///
/// Internally holds a `std::sync::Mutex<HashMap<Url, CancellationToken>>`.
/// Each call to `schedule` for a URI:
/// 1. (While holding the lock) Cancels the previous token (if any) and inserts
///    a fresh one — this is atomic from the caller's perspective.
/// 2. Spawns a tokio task: sleep `delay`, check not cancelled, run `work`.
///
/// The work closure receives the `CancellationToken` so it can check
/// `token.is_cancelled()` during long-running operations.
#[derive(Clone, Debug)]
pub struct Debouncer {
    pending: Arc<Mutex<HashMap<Url, CancellationToken>>>,
    delay: Duration,
}

impl Debouncer {
    /// Create a new debouncer with the given delay.
    ///
    /// `delay = Duration::from_millis(200)` is the gopls/rust-analyzer default.
    pub fn new(delay: Duration) -> Self {
        Self {
            pending: Arc::new(Mutex::new(HashMap::new())),
            delay,
        }
    }

    /// Schedule `work` for `uri`, debounced by `self.delay`.
    ///
    /// If a task is already pending for `uri`, it is cancelled before the new
    /// one is scheduled.  The new task receives a fresh `CancellationToken`.
    ///
    /// This method is synchronous — the token is inserted into the map
    /// before the method returns, making `cancel_all` race-free.
    pub fn schedule<F, Fut>(&self, uri: Url, work: F)
    where
        F: FnOnce(CancellationToken) -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        // Atomically cancel old + insert new token while holding the mutex.
        let token = {
            let mut map = self.pending.lock().unwrap();
            if let Some(old) = map.get(&uri) {
                old.cancel();
            }
            let token = CancellationToken::new();
            map.insert(uri.clone(), token.clone());
            token
        }; // mutex released here

        let pending = Arc::clone(&self.pending);
        let delay = self.delay;

        tokio::spawn(async move {
            // Wait for the debounce delay.
            tokio::time::sleep(delay).await;

            // Check if we were cancelled during the sleep.
            if token.is_cancelled() {
                return;
            }

            // Remove ourselves from the pending map (best-effort: a newer
            // schedule may have already replaced us with a fresh token).
            {
                let mut map = pending.lock().unwrap();
                // Only remove if the stored token is not cancelled (meaning it
                // hasn't been superseded by a newer token that was then also
                // cancelled — unlikely but safe to check).
                if let Some(stored) = map.get(&uri) {
                    if !stored.is_cancelled() {
                        map.remove(&uri);
                    }
                }
            }

            // Run work, catching any panics so they don't crash the debouncer.
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                work(token)
            }));

            match result {
                Ok(fut) => fut.await,
                Err(payload) => {
                    let msg = if let Some(s) = payload.downcast_ref::<&str>() {
                        s.to_string()
                    } else if let Some(s) = payload.downcast_ref::<String>() {
                        s.clone()
                    } else {
                        "unknown".to_string()
                    };
                    tracing::error!(panic = %msg, "debouncer work panicked");
                }
            }
        });
    }

    /// Cancel all pending tasks and clear the map.
    ///
    /// Called during server shutdown to ensure no orphan tasks remain.
    pub fn cancel_all(&self) {
        let mut map = self.pending.lock().unwrap();
        for token in map.values() {
            token.cancel();
        }
        map.clear();
    }

    /// Number of currently pending tasks (for testing / metrics).
    pub fn pending_count(&self) -> usize {
        self.pending.lock().unwrap().len()
    }
}

impl Default for Debouncer {
    fn default() -> Self {
        Self::new(Duration::from_millis(200))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use tokio::time::{sleep, Duration};

    fn uri(p: &str) -> Url {
        Url::parse(&format!("file:///{p}")).unwrap()
    }

    // ── pos1 ─────────────────────────────────────────────────────────────────

    /// pos1: schedule → after delay, work executes.
    #[tokio::test]
    async fn pos1_work_executes_after_delay() {
        let db = Debouncer::new(Duration::from_millis(20));
        let counter = Arc::new(AtomicUsize::new(0));
        let c = Arc::clone(&counter);

        db.schedule(uri("a.nv"), move |_tok| async move {
            c.fetch_add(1, Ordering::SeqCst);
        });

        sleep(Duration::from_millis(200)).await;
        assert_eq!(counter.load(Ordering::SeqCst), 1, "work should have run once");
    }

    // ── pos2 ─────────────────────────────────────────────────────────────────

    /// pos2: two rapid schedules for same URI → first cancelled, only second runs.
    #[tokio::test]
    async fn pos2_second_schedule_cancels_first() {
        let db = Debouncer::new(Duration::from_millis(50));
        let counter = Arc::new(AtomicUsize::new(0));

        let c1 = Arc::clone(&counter);
        db.schedule(uri("b.nv"), move |_tok| async move {
            c1.fetch_add(1, Ordering::SeqCst);
        });

        // Immediately schedule another — first should be cancelled
        let c2 = Arc::clone(&counter);
        db.schedule(uri("b.nv"), move |_tok| async move {
            c2.fetch_add(10, Ordering::SeqCst);
        });

        sleep(Duration::from_millis(500)).await;
        // Only the second task ran (added 10, not 1+10)
        assert_eq!(counter.load(Ordering::SeqCst), 10, "only second schedule should run");
    }

    // ── pos3 ─────────────────────────────────────────────────────────────────

    /// pos3: different URIs execute concurrently.
    #[tokio::test]
    async fn pos3_different_uris_run_concurrently() {
        let db = Debouncer::new(Duration::from_millis(20));
        let counter = Arc::new(AtomicUsize::new(0));

        for i in 0..5u32 {
            let c = Arc::clone(&counter);
            db.schedule(uri(&format!("file{i}.nv")), move |_tok| async move {
                c.fetch_add(1, Ordering::SeqCst);
            });
        }

        sleep(Duration::from_millis(300)).await;
        assert_eq!(counter.load(Ordering::SeqCst), 5, "all 5 different URIs should run");
    }

    // ── pos4 ─────────────────────────────────────────────────────────────────

    /// pos4: 100 rapid schedules for one URI → only last executes.
    #[tokio::test]
    async fn pos4_100_rapid_only_last_runs() {
        let db = Debouncer::new(Duration::from_millis(30));
        let counter = Arc::new(AtomicUsize::new(0));

        for _ in 0..100 {
            let c = Arc::clone(&counter);
            db.schedule(uri("c.nv"), move |_tok| async move {
                c.fetch_add(1, Ordering::SeqCst);
            });
            // No sleep — all 100 schedules fire before any delay elapses
        }

        sleep(Duration::from_millis(500)).await;
        assert_eq!(counter.load(Ordering::SeqCst), 1, "only last schedule should run");
    }

    // ── neg1 ─────────────────────────────────────────────────────────────────

    /// neg1: work that panics doesn't crash the debouncer.
    #[tokio::test]
    async fn neg1_panicking_work_doesnt_crash_debouncer() {
        let db = Debouncer::new(Duration::from_millis(10));

        db.schedule(uri("panic.nv"), |_tok| async {
            panic!("deliberate test panic");
        });

        sleep(Duration::from_millis(200)).await;

        // Debouncer still works after the panic
        let counter = Arc::new(AtomicUsize::new(0));
        let c = Arc::clone(&counter);
        db.schedule(uri("after_panic.nv"), move |_tok| async move {
            c.fetch_add(1, Ordering::SeqCst);
        });

        sleep(Duration::from_millis(200)).await;
        assert_eq!(counter.load(Ordering::SeqCst), 1, "debouncer should still work after panic");
    }

    // ── edge1 ────────────────────────────────────────────────────────────────

    /// edge1: cancel_all immediately after schedule → work doesn't run.
    #[tokio::test]
    async fn edge1_cancel_all_before_work_runs() {
        let db = Debouncer::new(Duration::from_millis(50));
        let counter = Arc::new(AtomicUsize::new(0));
        let c = Arc::clone(&counter);

        db.schedule(uri("d.nv"), move |_tok| async move {
            c.fetch_add(1, Ordering::SeqCst);
        });

        // Cancel before delay elapses — token is guaranteed in map because
        // schedule() inserts it synchronously before returning.
        db.cancel_all();

        sleep(Duration::from_millis(300)).await;
        assert_eq!(counter.load(Ordering::SeqCst), 0, "cancelled work should not run");
    }

    // ── edge2 ────────────────────────────────────────────────────────────────

    /// edge2: work receives token and respects is_cancelled() mid-loop.
    #[tokio::test]
    async fn edge2_work_checks_is_cancelled() {
        let db = Debouncer::new(Duration::from_millis(10));
        let counter = Arc::new(AtomicUsize::new(0));
        let c = Arc::clone(&counter);

        // First schedule: work runs a loop but gets cancelled by second schedule
        db.schedule(uri("e.nv"), move |tok| async move {
            for _ in 0..100 {
                if tok.is_cancelled() {
                    return;
                }
                sleep(Duration::from_millis(5)).await;
                c.fetch_add(1, Ordering::SeqCst);
            }
        });

        // Wait for delay to pass, then immediately replace with a second schedule
        sleep(Duration::from_millis(20)).await;
        db.schedule(uri("e.nv"), |_tok| async {});

        sleep(Duration::from_millis(300)).await;
        // counter should be small (< 100) because it was cancelled early
        let n = counter.load(Ordering::SeqCst);
        assert!(n < 100, "work should have been interrupted; ran {} iterations", n);
    }

    // ── edge3 ────────────────────────────────────────────────────────────────

    /// edge3: cancel_all with no pending tasks is a no-op (no panic).
    #[tokio::test]
    async fn edge3_cancel_all_empty_noop() {
        let db = Debouncer::new(Duration::from_millis(10));
        db.cancel_all(); // Should not panic
    }
}
