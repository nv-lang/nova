//! Document cache integration tests for nova-lsp — Plan 104.0.3.
//!
//! Spawns the real nova-lsp binary and sends textDocument/did* notifications
//! over JSON-RPC stdio.  Tests verify:
//!   - Server handles each notification without crashing or hanging.
//!   - Server remains responsive to subsequent lifecycle requests.
//!   - Warning logs appear in stderr for protocol violations (neg cases).
//!
//! State correctness (text matches, version incremented, document removed) is
//! separately verified by unit tests in `src/state.rs`.
//!
//! Tests:
//! - pos1: didOpen  → server alive, responds to shutdown
//! - pos2: didChange (after open) → version increment → server alive
//! - pos3: didClose (after open) → server alive
//! - neg1: didChange on never-opened URI → ignored, no crash, warn logged
//! - neg2: didOpen same URI twice → overwrite, server alive, warn logged

use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

// ─────────────────────────────────────────────────────────────────────────────
// LspProcess helper (with stderr capture for warning verification)
// ─────────────────────────────────────────────────────────────────────────────

struct LspProcess {
    child: Child,
    stdin: Option<ChildStdin>,
    reader: BufReader<ChildStdout>,
    /// Receives the accumulated stderr text after the process exits.
    stderr_rx: mpsc::Receiver<String>,
    next_id: u64,
}

impl LspProcess {
    fn spawn() -> Self {
        let binary = std::path::PathBuf::from(env!("CARGO_BIN_EXE_nova-lsp"));
        let mut child = Command::new(&binary)
            // Enable info+warn logging so warning messages appear in stderr.
            .env("NOVA_LSP_LOG", "nova_lsp=debug,warn")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap_or_else(|e| panic!("failed to spawn nova-lsp ({binary:?}): {e}"));

        let stdin = child.stdin.take().expect("stdin piped");
        let stdout = child.stdout.take().expect("stdout piped");
        let mut stderr = child.stderr.take().expect("stderr piped");

        // Drain stderr in a background thread so the process never blocks on a
        // full pipe buffer, and so we can inspect it after exit.
        let (stderr_tx, stderr_rx) = mpsc::channel::<String>();
        thread::spawn(move || {
            let mut buf = String::new();
            let _ = stderr.read_to_string(&mut buf);
            let _ = stderr_tx.send(buf);
        });

        LspProcess {
            child,
            stdin: Some(stdin),
            reader: BufReader::new(stdout),
            stderr_rx,
            next_id: 1,
        }
    }

    fn write_message(&mut self, msg: &serde_json::Value) {
        let stdin = self.stdin.as_mut().expect("stdin already closed");
        let body = msg.to_string();
        write!(stdin, "Content-Length: {}\r\n\r\n", body.len()).expect("write header");
        stdin.write_all(body.as_bytes()).expect("write body");
        stdin.flush().expect("flush");
    }

    fn send_request(&mut self, method: &str, params: serde_json::Value) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.write_message(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        }));
        id
    }

    fn send_notification(&mut self, method: &str, params: serde_json::Value) {
        self.write_message(&serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        }));
    }

    fn read_response(&mut self) -> serde_json::Value {
        let mut content_length: usize = 0;
        loop {
            let mut line = String::new();
            self.reader.read_line(&mut line).expect("read header");
            assert!(!line.is_empty(), "stdout closed unexpectedly");
            if line == "\r\n" || line == "\n" {
                break;
            }
            if let Some(rest) = line.strip_prefix("Content-Length:") {
                content_length = rest.trim().parse().expect("parse content-length");
            }
        }
        assert!(content_length > 0, "response with Content-Length: 0");
        let mut body = vec![0u8; content_length];
        self.reader.read_exact(&mut body).expect("read body");
        serde_json::from_slice(&body).expect("parse JSON response")
    }

    fn request_no_params(&mut self, method: &str) -> serde_json::Value {
        let id = self.next_id;
        self.next_id += 1;
        self.write_message(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
        }));
        let resp = self.read_response();
        assert_eq!(resp["id"].as_u64(), Some(id), "response id mismatch");
        resp
    }

    fn initialize(&mut self) {
        let id = self.send_request(
            "initialize",
            serde_json::json!({
                "processId": null,
                "rootUri": null,
                "capabilities": {},
            }),
        );
        let resp = self.read_response();
        assert_eq!(resp["id"].as_u64(), Some(id));
        assert!(resp.get("error").is_none(), "initialize failed: {resp}");
        self.send_notification("initialized", serde_json::json!({}));
    }

    fn did_open(&mut self, uri: &str, text: &str, version: i32) {
        self.send_notification(
            "textDocument/didOpen",
            serde_json::json!({
                "textDocument": {
                    "uri": uri,
                    "languageId": "nova",
                    "version": version,
                    "text": text,
                }
            }),
        );
    }

    fn did_change(&mut self, uri: &str, text: &str, version: i32) {
        self.send_notification(
            "textDocument/didChange",
            serde_json::json!({
                "textDocument": { "uri": uri, "version": version },
                "contentChanges": [{ "text": text }]
            }),
        );
    }

    fn did_close(&mut self, uri: &str) {
        self.send_notification(
            "textDocument/didClose",
            serde_json::json!({
                "textDocument": { "uri": uri }
            }),
        );
    }

    /// Verify server is alive by sending shutdown + exit, return collected stderr.
    fn shutdown_and_collect_stderr(mut self) -> (bool, String) {
        let shutdown_resp = self.request_no_params("shutdown");
        let shutdown_ok = shutdown_resp.get("error").is_none();

        self.send_notification("exit", serde_json::Value::Null);
        drop(self.stdin.take());

        // Wait for process to exit
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(_status) = self.child.try_wait().expect("wait failed") {
                break;
            }
            if Instant::now() >= deadline {
                let _ = self.child.kill();
                break;
            }
            thread::sleep(Duration::from_millis(50));
        }

        // Collect stderr (drain thread should be done now that process exited)
        let stderr = self
            .stderr_rx
            .recv_timeout(Duration::from_secs(5))
            .unwrap_or_default();

        (shutdown_ok, stderr)
    }
}

impl Drop for LspProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

/// pos1: didOpen → document is cached; server remains alive and responsive.
///
/// State verification (text matches, version correct) is in `src/state.rs`
/// unit tests.  Here we verify protocol correctness: server does not crash,
/// does not emit an error response, and responds normally to shutdown.
#[test]
fn pos1_did_open_server_alive() {
    let mut lsp = LspProcess::spawn();
    lsp.initialize();

    // Open a Nova source file
    lsp.did_open(
        "file:///workspace/main.nv",
        "fn main() => println(\"hello\")",
        1,
    );

    // Server must still be alive and accept shutdown
    let (ok, _stderr) = lsp.shutdown_and_collect_stderr();
    assert!(ok, "server crashed or shutdown failed after didOpen");
}

/// pos2: didOpen then didChange → version incremented; server alive.
///
/// We send two sequential edits (version 1 → 2 → 3) to verify the server
/// handles multiple changes without error.  The version increment is also
/// verified in the unit test `pos2_change_updates_text_and_version`.
#[test]
fn pos2_did_change_updates_version_server_alive() {
    let mut lsp = LspProcess::spawn();
    lsp.initialize();

    let uri = "file:///workspace/counter.nv";

    // Open at version 1
    lsp.did_open(uri, "fn counter() => 0", 1);

    // Change to version 2
    lsp.did_change(uri, "fn counter() => 1", 2);

    // Change again to version 3 (multiple sequential changes must be handled)
    lsp.did_change(uri, "fn counter() => 2", 3);

    let (ok, _stderr) = lsp.shutdown_and_collect_stderr();
    assert!(ok, "server crashed or shutdown failed after didChange x2");
}

/// pos3: didOpen then didClose → document evicted; server alive.
#[test]
fn pos3_did_close_evicts_document() {
    let mut lsp = LspProcess::spawn();
    lsp.initialize();

    let uri = "file:///workspace/temp.nv";
    lsp.did_open(uri, "let x int = 1", 1);
    lsp.did_close(uri);

    let (ok, _stderr) = lsp.shutdown_and_collect_stderr();
    assert!(ok, "server crashed or shutdown failed after didClose");
}

/// neg1: didChange on a URI that was never opened → ignored gracefully.
///
/// The server MUST NOT crash or hang.  It should log a warning and continue
/// serving other requests.  The warning is visible in stderr when
/// NOVA_LSP_LOG=nova_lsp=debug (which we set in LspProcess::spawn).
#[test]
fn neg1_did_change_unopened_graceful() {
    let mut lsp = LspProcess::spawn();
    lsp.initialize();

    // Send didChange without a prior didOpen (protocol violation by client)
    lsp.did_change("file:///workspace/never_opened.nv", "fn x() => ()", 1);

    let (ok, stderr) = lsp.shutdown_and_collect_stderr();
    assert!(
        ok,
        "server crashed after didChange on unopened document:\n{stderr}"
    );

    // Verify the server emitted a warning (not an error — protocol violations
    // from the client must be logged, not propagated as errors to the client).
    assert!(
        stderr.contains("WARN") || stderr.contains("warn"),
        "expected a WARN log for didChange on unopened document\nstderr:\n{stderr}"
    );
}

/// neg2: didOpen for the same URI twice → second overwrites; warning logged.
///
/// This is a protocol violation by the client (LSP spec requires exactly one
/// didOpen per document lifetime).  The server must handle it defensively:
/// overwrite the cached text and log a warning.
#[test]
fn neg2_did_open_twice_overwrite_and_warn() {
    let mut lsp = LspProcess::spawn();
    lsp.initialize();

    let uri = "file:///workspace/duplicate.nv";

    // First open — normal
    lsp.did_open(uri, "fn first() => ()", 1);

    // Second open — protocol violation; server should overwrite and warn
    lsp.did_open(uri, "fn second() => ()", 2);

    let (ok, stderr) = lsp.shutdown_and_collect_stderr();
    assert!(
        ok,
        "server crashed after double didOpen:\n{stderr}"
    );

    // Verify the overwrite warning is logged
    assert!(
        stderr.contains("WARN") || stderr.contains("warn"),
        "expected a WARN log for duplicate didOpen\nstderr:\n{stderr}"
    );
    // More specifically, the warning should mention "overwriting"
    assert!(
        stderr.to_lowercase().contains("overwrit"),
        "expected 'overwrite' in warning message\nstderr:\n{stderr}"
    );
}
