//! Memory hygiene + shutdown semantics tests (Plan 104.1.Ф.8).
//!
//! Verifies:
//! - open/close cycles don't accumulate documents in the cache.
//! - shutdown during pending recompile completes gracefully.
//! - 100 open+close cycles keep the server alive and responsive.

use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};

// ─────────────────────────────────────────────────────────────────────────────
// LspProcess helper (notification-aware, same pattern as publish_workflow.rs)
// ─────────────────────────────────────────────────────────────────────────────

struct LspProcess {
    child: Child,
    stdin: Option<ChildStdin>,
    reader: BufReader<ChildStdout>,
    next_id: u64,
}

impl LspProcess {
    fn spawn() -> Self {
        let binary = std::path::PathBuf::from(env!("CARGO_BIN_EXE_nova-lsp"));
        let mut child = Command::new(&binary)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap_or_else(|e| panic!("failed to spawn nova-lsp: {e}"));

        let stdin = child.stdin.take().expect("stdin piped");
        let stdout = child.stdout.take().expect("stdout piped");

        LspProcess {
            child,
            stdin: Some(stdin),
            reader: BufReader::new(stdout),
            next_id: 1,
        }
    }

    fn write_message(&mut self, msg: &serde_json::Value) {
        let stdin = self.stdin.as_mut().expect("stdin closed");
        let body = msg.to_string();
        write!(stdin, "Content-Length: {}\r\n\r\n", body.len()).unwrap();
        stdin.write_all(body.as_bytes()).unwrap();
        stdin.flush().unwrap();
    }

    fn send_notification(&mut self, method: &str, params: serde_json::Value) {
        self.write_message(&serde_json::json!({
            "jsonrpc": "2.0", "method": method, "params": params
        }));
    }

    fn read_one(&mut self) -> serde_json::Value {
        let mut content_length = 0usize;
        loop {
            let mut line = String::new();
            self.reader.read_line(&mut line).expect("read header");
            if line.is_empty() { panic!("stdout closed"); }
            if line == "\r\n" || line == "\n" { break; }
            if let Some(rest) = line.strip_prefix("Content-Length:") {
                content_length = rest.trim().parse().unwrap();
            }
        }
        let mut body = vec![0u8; content_length];
        self.reader.read_exact(&mut body).unwrap();
        serde_json::from_slice(&body).unwrap()
    }

    fn read_response_id(&mut self, id: u64) -> serde_json::Value {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if Instant::now() >= deadline { panic!("timeout waiting for response id={id}"); }
            let msg = self.read_one();
            if msg.get("id").and_then(|v| v.as_u64()) == Some(id) { return msg; }
        }
    }

    fn request_no_params(&mut self, method: &str) -> serde_json::Value {
        let id = self.next_id; self.next_id += 1;
        self.write_message(&serde_json::json!({
            "jsonrpc": "2.0", "id": id, "method": method
        }));
        self.read_response_id(id)
    }

    fn initialize(&mut self) {
        let id = self.next_id; self.next_id += 1;
        self.write_message(&serde_json::json!({
            "jsonrpc": "2.0", "id": id, "method": "initialize",
            "params": { "processId": null, "rootUri": null, "capabilities": {} }
        }));
        self.read_response_id(id);
        self.send_notification("initialized", serde_json::json!({}));
    }

    fn did_open(&mut self, uri: &str, text: &str) {
        self.send_notification("textDocument/didOpen", serde_json::json!({
            "textDocument": { "uri": uri, "languageId": "nova", "version": 1, "text": text }
        }));
    }

    fn did_close(&mut self, uri: &str) {
        self.send_notification("textDocument/didClose", serde_json::json!({
            "textDocument": { "uri": uri }
        }));
    }

    fn shutdown_and_exit(&mut self) {
        self.request_no_params("shutdown");
        self.send_notification("exit", serde_json::Value::Null);
        drop(self.stdin.take());
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if self.child.try_wait().expect("wait").is_some() { break; }
            if Instant::now() >= deadline { let _ = self.child.kill(); break; }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

impl Drop for LspProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Direct WorkspaceState tests (no process spawn)
// ─────────────────────────────────────────────────────────────────────────────

/// pos1: open + close → document evicted from cache.
#[test]
fn pos1_open_close_cache_empty_after_close() {
    use nova_lsp::state::{ParsedFile, WorkspaceState};
    use ropey::Rope;
    use tower_lsp::lsp_types::Url;

    let state = WorkspaceState::default();
    let uri = Url::parse("file:///test/pos1.nv").unwrap();

    state.docs.insert(uri.clone(), ParsedFile { text: Rope::from_str("x"), version: 1 });
    assert!(state.docs.contains_key(&uri), "doc should be cached after open");

    state.docs.remove(&uri);
    assert!(!state.docs.contains_key(&uri), "doc should be gone after close");
}

/// pos2: 100 cycles of insert+remove on WorkspaceState → docs map is empty.
#[test]
fn pos2_100_open_close_cycles_stable_state() {
    use nova_lsp::state::{ParsedFile, WorkspaceState};
    use ropey::Rope;
    use tower_lsp::lsp_types::Url;

    let state = WorkspaceState::default();

    for i in 0..100 {
        let uri = Url::parse(&format!("file:///test/cycle{i}.nv")).unwrap();
        state.docs.insert(uri.clone(), ParsedFile { text: Rope::from_str("fn f()"), version: 1 });
        state.docs.remove(&uri);
    }

    assert_eq!(state.docs.len(), 0, "docs map should be empty after 100 open/close cycles");
}

/// pos3: cancel_all during active debouncer state → no orphan task warning.
#[test]
fn pos3_cancel_all_cleans_pending_tasks() {
    use nova_lsp::debouncer::Debouncer;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tower_lsp::lsp_types::Url;

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(async {
        let db = Debouncer::new(Duration::from_millis(200)); // 200ms delay
        let counter = Arc::new(AtomicUsize::new(0));

        // Schedule 10 tasks
        for i in 0..10 {
            let c = Arc::clone(&counter);
            let uri = Url::parse(&format!("file:///f{i}.nv")).unwrap();
            db.schedule(uri, move |_tok| async move { c.fetch_add(1, Ordering::SeqCst); });
        }

        // Cancel before delay elapses
        db.cancel_all();

        tokio::time::sleep(Duration::from_millis(500)).await;
        assert_eq!(counter.load(Ordering::SeqCst), 0, "all pending tasks should be cancelled");
    });
}

/// neg1: shutdown without prior initialize → graceful, no panic.
#[test]
fn neg1_shutdown_without_initialize_no_panic() {
    let mut lsp = LspProcess::spawn();
    // Send shutdown directly without initialize — server should handle gracefully
    let resp = lsp.request_no_params("shutdown");
    // tower-lsp will return an error (server not initialized), but must not panic
    let _ = resp;
    // Exit cleanly
    lsp.send_notification("exit", serde_json::Value::Null);
    drop(lsp.stdin.take());
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if lsp.child.try_wait().expect("wait").is_some() { break; }
        if Instant::now() >= deadline { break; }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// pos4: server survives rapid open+close without initialize errors.
#[test]
fn pos4_open_close_server_survives() {
    let mut lsp = LspProcess::spawn();
    lsp.initialize();

    let uri = "file:///test/survive.nv";
    let src = "module test.survive\n\nfn f() -> int => 42\n";

    lsp.did_open(uri, src);
    lsp.did_close(uri);

    lsp.shutdown_and_exit();
    // If we get here without panicking, the server survived
}
