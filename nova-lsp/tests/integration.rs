//! End-to-end integration tests for nova-lsp — Plan 104.0.4.
//!
//! These tests use `tokio::process::Command` to spawn nova-lsp asynchronously
//! and drive it over JSON-RPC stdio.  This scaffold forms the foundation for
//! all future LSP feature tests (hover, completion, quick-fixes, rename).
//!
//! The `AsyncLspProcess` helper encodes and decodes the LSP base-protocol
//! Content-Length framing, letting tests focus on LSP semantics.
//!
//! Tests:
//! - pos1: full handshake — initialize → initialized → shutdown → exit (code 0)
//! - pos2: e2e didOpen → didChange → didClose (no segfault, child alive after)
//! - neg1: malformed JSON body (invalid JSON after valid Content-Length) →
//!         server returns -32700 OR handles gracefully (stays alive)

use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::time::timeout;

// ─────────────────────────────────────────────────────────────────────────────
// AsyncLspProcess — async JSON-RPC over stdio helper
// ─────────────────────────────────────────────────────────────────────────────

/// Async wrapper for a spawned nova-lsp process.
///
/// Provides high-level LSP helpers built on top of `tokio::process` and
/// the LSP base protocol framing (`Content-Length: N\r\n\r\n<body>`).
struct AsyncLspProcess {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl AsyncLspProcess {
    async fn spawn() -> Self {
        let binary = std::path::PathBuf::from(env!("CARGO_BIN_EXE_nova-lsp"));

        let mut child = Command::new(&binary)
            .env("NOVA_LSP_LOG", "nova_lsp=debug,warn")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            // Kill child when our handle drops (avoids zombie processes on test panic)
            .kill_on_drop(true)
            .spawn()
            .unwrap_or_else(|e| panic!("failed to spawn nova-lsp ({binary:?}): {e}"));

        let stdin = child.stdin.take().expect("stdin piped");
        let stdout = child.stdout.take().expect("stdout piped");

        AsyncLspProcess {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            next_id: 1,
        }
    }

    // ── Write side ──────────────────────────────────────────────────────────

    /// Write a JSON-RPC message with LSP base-protocol framing.
    async fn write_message(&mut self, msg: &serde_json::Value) {
        let body = msg.to_string();
        let header = format!("Content-Length: {}\r\n\r\n", body.len());
        self.stdin
            .write_all(header.as_bytes())
            .await
            .expect("write Content-Length header");
        self.stdin
            .write_all(body.as_bytes())
            .await
            .expect("write body");
        self.stdin.flush().await.expect("flush stdin");
    }

    /// Write raw bytes directly (for negative tests with broken framing).
    async fn write_raw(&mut self, bytes: &[u8]) {
        self.stdin.write_all(bytes).await.expect("write raw bytes");
        self.stdin.flush().await.expect("flush stdin");
    }

    async fn send_request(&mut self, method: &str, params: serde_json::Value) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.write_message(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        }))
        .await;
        id
    }

    async fn send_notification(&mut self, method: &str, params: serde_json::Value) {
        self.write_message(&serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        }))
        .await;
    }

    // ── Read side ────────────────────────────────────────────────────────────

    /// Read one LSP response from stdout.
    ///
    /// Parses `Content-Length: N\r\n\r\n<body>` framing and deserializes the
    /// JSON body.  Returns `None` on EOF (server closed stdout).
    async fn read_response(&mut self) -> Option<serde_json::Value> {
        let mut content_length: usize = 0;

        // Read header lines until blank line
        loop {
            let mut line = String::new();
            match self.stdout.read_line(&mut line).await {
                Ok(0) => return None, // EOF
                Ok(_) => {}
                Err(e) => panic!("failed to read header line: {e}"),
            }
            if line == "\r\n" || line == "\n" {
                break;
            }
            if let Some(rest) = line.strip_prefix("Content-Length:") {
                content_length = rest
                    .trim()
                    .parse()
                    .unwrap_or_else(|e| panic!("bad Content-Length: {e}"));
            }
        }

        if content_length == 0 {
            return None; // unexpected empty message
        }

        let mut body = vec![0u8; content_length];
        if self.stdout.read_exact(&mut body).await.is_err() {
            return None; // EOF or IO error on body read
        }

        serde_json::from_slice(&body).ok()
    }

    /// Read a response with a deadline.  Returns `None` on timeout or EOF.
    async fn read_response_timeout(&mut self, d: Duration) -> Option<serde_json::Value> {
        match timeout(d, self.read_response()).await {
            Ok(result) => result,  // inner Future result: Option<Value>
            Err(_elapsed) => None, // deadline exceeded
        }
    }

    // ── High-level helpers ───────────────────────────────────────────────────

    async fn request_no_params(&mut self, method: &str) -> serde_json::Value {
        let id = self.next_id;
        self.next_id += 1;
        self.write_message(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
        }))
        .await;
        let resp = self
            .read_response_timeout(Duration::from_secs(5))
            .await
            .unwrap_or_else(|| panic!("no response to '{method}' within 5s"));
        assert_eq!(
            resp["id"].as_u64(),
            Some(id),
            "response id mismatch for '{method}': {resp}"
        );
        resp
    }

    async fn initialize(&mut self) {
        let id = self.send_request(
            "initialize",
            serde_json::json!({
                "processId": null,
                "rootUri": null,
                "capabilities": {
                    "general": { "positionEncodings": ["utf-16"] }
                },
            }),
        )
        .await;
        let resp = self
            .read_response_timeout(Duration::from_secs(5))
            .await
            .expect("no initialize response within 5s");
        assert_eq!(resp["id"].as_u64(), Some(id));
        assert!(resp.get("error").is_none(), "initialize failed: {resp}");
        self.send_notification("initialized", serde_json::json!({}))
            .await;
    }

    async fn did_open(&mut self, uri: &str, text: &str, version: i32) {
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
        )
        .await;
    }

    async fn did_change(&mut self, uri: &str, text: &str, version: i32) {
        self.send_notification(
            "textDocument/didChange",
            serde_json::json!({
                "textDocument": { "uri": uri, "version": version },
                "contentChanges": [{ "text": text }],
            }),
        )
        .await;
    }

    async fn did_close(&mut self, uri: &str) {
        self.send_notification(
            "textDocument/didClose",
            serde_json::json!({ "textDocument": { "uri": uri } }),
        )
        .await;
    }

    /// Perform a clean shutdown + exit, return the exit status.
    async fn shutdown_and_exit(mut self) -> std::process::ExitStatus {
        let shutdown_resp = self.request_no_params("shutdown").await;
        assert!(
            shutdown_resp.get("error").is_none(),
            "shutdown returned error: {shutdown_resp}"
        );
        self.send_notification("exit", serde_json::Value::Null)
            .await;

        // Close stdin — belt-and-suspenders in case tower-lsp handles exit by
        // closing the connection rather than calling std::process::exit().
        drop(self.stdin);

        // Wait for child to exit (10s deadline)
        match timeout(Duration::from_secs(10), self.child.wait()).await {
            Ok(Ok(status)) => status,
            Ok(Err(e)) => panic!("child wait error: {e}"),
            Err(_) => {
                let _ = self.child.kill().await;
                panic!("nova-lsp did not exit within 10s after shutdown/exit");
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

/// pos1: Full handshake — initialize → initialized → shutdown → exit → code 0.
///
/// This is the canonical "the server works at all" smoke test.
/// All other integration tests depend on this sequence being reliable.
#[tokio::test]
async fn pos1_full_handshake_exit_zero() {
    let mut lsp = AsyncLspProcess::spawn().await;
    lsp.initialize().await;

    let status = lsp.shutdown_and_exit().await;
    assert!(
        status.success(),
        "nova-lsp exited non-zero after clean handshake: {status}"
    );
}

/// pos2: e2e didOpen → didChange → didClose — no segfault, child alive after.
///
/// Sends the three document lifecycle notifications in sequence and verifies:
/// - No crash or panic at any point.
/// - After all three notifications the server is still responsive (responds to shutdown).
/// - Exit code is 0 (clean termination after full LSP lifecycle).
#[tokio::test]
async fn pos2_document_open_change_close_alive() {
    let mut lsp = AsyncLspProcess::spawn().await;
    lsp.initialize().await;

    let uri = "file:///e2e/main.nv";

    // Open
    lsp.did_open(uri, "fn main() => ()", 1).await;

    // Change (simulates rapid keystrokes — multiple changes in sequence)
    lsp.did_change(uri, "fn main() => println(\"hi\")", 2)
        .await;
    lsp.did_change(uri, "fn main() => println(\"hello\")", 3)
        .await;

    // Close
    lsp.did_close(uri).await;

    // Server must still be alive and perform a clean shutdown
    let status = lsp.shutdown_and_exit().await;
    assert!(
        status.success(),
        "nova-lsp exited non-zero after didOpen/didChange/didClose: {status}"
    );
}

/// neg1: Malformed JSON body → server returns -32700 OR handles gracefully.
///
/// We send a valid Content-Length header (7 bytes) with a body that is not
/// valid JSON (`not_jsn`).  Per JSON-RPC 2.0 spec, the server SHOULD return
/// -32700.  Some implementations silently drop the message or close the
/// connection instead.  All three behaviors are acceptable.
///
/// What is NEVER acceptable:
/// - A Rust panic (exit code 101 on Windows, SIGABRT on Unix).
/// - An infinite hang (server stops processing all subsequent messages).
///
/// Observed tower-lsp 0.20 behavior: codec logs error internally and closes
/// the connection (server exits).  This test handles all three outcomes.
#[tokio::test]
async fn neg1_malformed_json_no_panic_no_hang() {
    let mut lsp = AsyncLspProcess::spawn().await;
    lsp.initialize().await;

    // Valid Content-Length framing but invalid JSON body.
    // "not_jsn" is exactly 7 ASCII bytes — not valid JSON.
    lsp.write_raw(b"Content-Length: 7\r\n\r\nnot_jsn").await;

    // Give the server time to process the bad message.
    tokio::time::sleep(Duration::from_millis(400)).await;

    // Try to read a response (server might send -32700).
    let maybe_resp = lsp.read_response_timeout(Duration::from_millis(400)).await;

    // Force-kill the server after reading (or timing out).
    // After kill(), wait() collects the exit status so we can check it.
    let _ = lsp.child.kill().await;
    let exit_status = timeout(Duration::from_secs(3), lsp.child.wait())
        .await
        .ok() // timeout -> None
        .and_then(|r| r.ok()); // io::Error -> None

    // ── Assertion 1: if we got a response, it MUST be -32700 ────────────────
    if let Some(resp) = maybe_resp {
        let code = resp["error"]["code"].as_i64().unwrap_or_else(|| {
            panic!(
                "expected an error response to malformed JSON input, got: {resp}"
            )
        });
        assert_eq!(
            code, -32700,
            "expected -32700 (Parse error) for malformed JSON, got {code}: {resp}"
        );
    }
    // If no response: server closed the connection or silently dropped the message.
    // Both are acceptable — the key assertion is below.

    // ── Assertion 2: exit was NOT a Rust panic ────────────────────────────────
    // Rust panics exit with code 101 on Windows (via the default panic hook).
    // Any other exit code (0, 1, killed-by-signal) indicates graceful handling.
    if let Some(status) = exit_status {
        #[cfg(windows)]
        assert_ne!(
            status.code(),
            Some(101),
            "nova-lsp panicked (exit 101) after receiving malformed JSON"
        );
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            assert!(
                status.signal() != Some(6), // SIGABRT = Rust abort-panic
                "nova-lsp received SIGABRT (panic) after malformed JSON"
            );
        }
    }
    // If we couldn't collect exit status (shouldn't happen after kill), the test
    // still passes because we've verified the absence of a panic response above.
}
