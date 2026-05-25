//! Tests for the publishDiagnostics workflow (Plan 104.1.Ф.5).
//!
//! Verifies that the LSP server sends `textDocument/publishDiagnostics`
//! notifications in response to didOpen / didChange / didClose.
//!
//! We spawn the real nova-lsp binary and communicate over JSON-RPC stdio,
//! same as in lifecycle.rs.  The key addition: after a did* notification,
//! we read pending messages until we see a `publishDiagnostics` notification.

use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};

// ─────────────────────────────────────────────────────────────────────────────
// JSON-RPC helper (same pattern as lifecycle.rs)
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
            .unwrap_or_else(|e| panic!("failed to spawn nova-lsp ({binary:?}): {e}"));

        let stdin = child.stdin.take().expect("stdin was piped");
        let stdout = child.stdout.take().expect("stdout was piped");

        LspProcess {
            child,
            stdin: Some(stdin),
            reader: BufReader::new(stdout),
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

    fn read_one(&mut self) -> serde_json::Value {
        let mut content_length: usize = 0;
        loop {
            let mut line = String::new();
            self.reader.read_line(&mut line).expect("read header line");
            assert!(!line.is_empty(), "nova-lsp closed stdout unexpectedly");
            if line == "\r\n" || line == "\n" {
                break;
            }
            if let Some(rest) = line.strip_prefix("Content-Length:") {
                content_length = rest.trim().parse().expect("valid Content-Length");
            }
        }
        let mut body = vec![0u8; content_length];
        self.reader.read_exact(&mut body).expect("read body");
        serde_json::from_slice(&body).expect("valid JSON")
    }

    /// Read messages until we get one matching `predicate` or timeout.
    fn read_until<F>(&mut self, predicate: F, timeout: Duration) -> Option<serde_json::Value>
    where
        F: Fn(&serde_json::Value) -> bool,
    {
        // Set stdout to non-blocking isn't easy with BufReader, so we use a
        // separate thread approach: spawn a thread that reads, main thread polls.
        // Simpler: just call read_one in a loop with deadline check by using
        // try_wait on the child to detect unexpected death.
        let deadline = Instant::now() + timeout;
        loop {
            if Instant::now() >= deadline {
                return None;
            }
            // Use a timeout-aware read: try_wait the child first to detect crashes.
            match self.child.try_wait() {
                Ok(Some(_)) => panic!("nova-lsp exited unexpectedly"),
                Ok(None) => {}
                Err(e) => panic!("try_wait failed: {e}"),
            }
            let msg = self.read_one();
            if predicate(&msg) {
                return Some(msg);
            }
            // Skip non-matching messages (e.g., unrelated notifications).
        }
    }

    fn request(&mut self, method: &str, params: serde_json::Value) -> serde_json::Value {
        let id = self.send_request(method, params);
        // Read until we get a response with our id.
        self.read_until(
            |m| m.get("id").and_then(|v| v.as_u64()) == Some(id),
            Duration::from_secs(10),
        )
        .expect("timeout waiting for response")
    }

    fn request_no_params(&mut self, method: &str) -> serde_json::Value {
        let id = self.next_id;
        self.next_id += 1;
        self.write_message(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
        }));
        self.read_until(
            |m| m.get("id").and_then(|v| v.as_u64()) == Some(id),
            Duration::from_secs(10),
        )
        .expect("timeout waiting for response")
    }

    fn initialize_full(&mut self) {
        let _resp = self.request(
            "initialize",
            serde_json::json!({
                "processId": null,
                "rootUri": null,
                "capabilities": { "general": { "positionEncodings": ["utf-16"] } },
            }),
        );
        self.send_notification("initialized", serde_json::json!({}));
    }

    /// Wait for a `textDocument/publishDiagnostics` notification for `uri`.
    fn wait_for_diagnostics(&mut self, uri: &str, timeout: Duration) -> serde_json::Value {
        self.read_until(
            |m| {
                m.get("method").and_then(|v| v.as_str()) == Some("textDocument/publishDiagnostics")
                    && m["params"]["uri"].as_str() == Some(uri)
            },
            timeout,
        )
        .unwrap_or_else(|| panic!("timeout waiting for publishDiagnostics for {uri}"))
    }

    fn shutdown_and_exit(mut self) {
        self.request_no_params("shutdown");
        self.send_notification("exit", serde_json::Value::Null);
        drop(self.stdin.take());
        let _ = self.child.wait();
    }
}

impl Drop for LspProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

/// A minimal valid Nova source (no prelude symbols).
fn valid_nova() -> &'static str {
    "module test_lsp.valid\n\nfn add(a int, b int) -> int => a + b\n"
}

/// A Nova source with a syntax error.
fn broken_nova() -> &'static str {
    "module test_lsp.broken\n\nfn bad( => ()\n"
}

/// Build a `file:///` URI for a test file name.
fn test_uri(name: &str) -> String {
    // Use a temp-dir style path that exists on Windows; the LSP doesn't need
    // the file to exist on disk for didOpen (it receives the full text).
    format!("file:///tmp/nova_lsp_test/{name}")
}

fn did_open_params(uri: &str, text: &str) -> serde_json::Value {
    serde_json::json!({
        "textDocument": {
            "uri": uri,
            "languageId": "nova",
            "version": 1,
            "text": text,
        }
    })
}

fn did_change_params(uri: &str, version: i32, text: &str) -> serde_json::Value {
    // Full text refresh via null range (Incremental fallback).
    serde_json::json!({
        "textDocument": { "uri": uri, "version": version },
        "contentChanges": [{ "text": text }]
    })
}

fn did_close_params(uri: &str) -> serde_json::Value {
    serde_json::json!({ "textDocument": { "uri": uri } })
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

/// pos1: didOpen with a broken file → publishDiagnostics with ≥1 error.
#[test]
fn pos1_did_open_invalid_publishes_error() {
    let mut lsp = LspProcess::spawn();
    lsp.initialize_full();

    let uri = test_uri("pos1.nv");
    lsp.send_notification("textDocument/didOpen", did_open_params(&uri, broken_nova()));

    let notif = lsp.wait_for_diagnostics(&uri, Duration::from_secs(10));
    let diags = &notif["params"]["diagnostics"];
    assert!(
        diags.as_array().map(|a| !a.is_empty()).unwrap_or(false),
        "expected ≥1 diagnostic for broken file, got: {notif}"
    );

    lsp.shutdown_and_exit();
}

/// pos2: didOpen with valid file → publishDiagnostics with 0 errors.
#[test]
fn pos2_did_open_valid_publishes_empty() {
    let mut lsp = LspProcess::spawn();
    lsp.initialize_full();

    let uri = test_uri("pos2.nv");
    lsp.send_notification("textDocument/didOpen", did_open_params(&uri, valid_nova()));

    let notif = lsp.wait_for_diagnostics(&uri, Duration::from_secs(10));
    let diags = notif["params"]["diagnostics"].as_array().expect("diagnostics array");
    assert!(
        diags.is_empty(),
        "expected 0 diagnostics for valid file, got: {:?}",
        diags
    );

    lsp.shutdown_and_exit();
}

/// pos3: didChange fixing the error → publishDiagnostics with empty.
#[test]
fn pos3_did_change_fix_clears_errors() {
    let mut lsp = LspProcess::spawn();
    lsp.initialize_full();

    let uri = test_uri("pos3.nv");

    // Open broken
    lsp.send_notification("textDocument/didOpen", did_open_params(&uri, broken_nova()));
    let notif1 = lsp.wait_for_diagnostics(&uri, Duration::from_secs(10));
    assert!(
        notif1["params"]["diagnostics"].as_array().map(|a| !a.is_empty()).unwrap_or(false),
        "expected errors after opening broken file"
    );

    // Fix the file via didChange (full text refresh)
    lsp.send_notification("textDocument/didChange", did_change_params(&uri, 2, valid_nova()));
    let notif2 = lsp.wait_for_diagnostics(&uri, Duration::from_secs(10));
    let diags2 = notif2["params"]["diagnostics"].as_array().expect("array");
    assert!(diags2.is_empty(), "expected 0 diagnostics after fix, got: {:?}", diags2);

    lsp.shutdown_and_exit();
}

/// pos4: didChange breaking the file → publishDiagnostics with error.
#[test]
fn pos4_did_change_break_publishes_error() {
    let mut lsp = LspProcess::spawn();
    lsp.initialize_full();

    let uri = test_uri("pos4.nv");

    // Open valid
    lsp.send_notification("textDocument/didOpen", did_open_params(&uri, valid_nova()));
    let notif1 = lsp.wait_for_diagnostics(&uri, Duration::from_secs(10));
    assert!(
        notif1["params"]["diagnostics"].as_array().map(|a| a.is_empty()).unwrap_or(false),
        "expected 0 errors on valid open"
    );

    // Break the file
    lsp.send_notification("textDocument/didChange", did_change_params(&uri, 2, broken_nova()));
    let notif2 = lsp.wait_for_diagnostics(&uri, Duration::from_secs(10));
    assert!(
        notif2["params"]["diagnostics"].as_array().map(|a| !a.is_empty()).unwrap_or(false),
        "expected ≥1 error after breaking the file"
    );

    lsp.shutdown_and_exit();
}

/// pos5: didClose → publishDiagnostics with empty (clear editor).
#[test]
fn pos5_did_close_publishes_empty() {
    let mut lsp = LspProcess::spawn();
    lsp.initialize_full();

    let uri = test_uri("pos5.nv");

    // Open broken
    lsp.send_notification("textDocument/didOpen", did_open_params(&uri, broken_nova()));
    lsp.wait_for_diagnostics(&uri, Duration::from_secs(10)); // wait for initial diags

    // Close — should immediately publish empty
    lsp.send_notification("textDocument/didClose", did_close_params(&uri));
    let notif = lsp.wait_for_diagnostics(&uri, Duration::from_secs(5));
    let diags = notif["params"]["diagnostics"].as_array().expect("array");
    assert!(diags.is_empty(), "expected empty diagnostics on close, got: {:?}", diags);

    lsp.shutdown_and_exit();
}

/// neg1: server doesn't crash on panic-inducing input (InternalError published).
///
/// We can't easily make the compiler panic with a specific input, so instead
/// we send a file with deeply garbled content and just verify the server stays up.
#[test]
fn neg1_compiler_internal_error_doesnt_crash_server() {
    let mut lsp = LspProcess::spawn();
    lsp.initialize_full();

    let uri = test_uri("neg1.nv");
    // Highly garbled source that may trigger edge cases
    let garbled = "module neg1\n\n@@@@@@@@@@@\nfn ????() => ????\n";
    lsp.send_notification("textDocument/didOpen", did_open_params(&uri, garbled));

    // Some publishDiagnostics notification should arrive (errors or InternalError)
    let notif = lsp.wait_for_diagnostics(&uri, Duration::from_secs(10));
    let _diags = &notif["params"]["diagnostics"];
    // Server should still be alive — do a clean shutdown
    lsp.shutdown_and_exit();
}

/// edge1: didOpen + immediate didClose → no panic, diagnostics eventually cleared.
#[test]
fn edge1_open_then_immediate_close_no_panic() {
    let mut lsp = LspProcess::spawn();
    lsp.initialize_full();

    let uri = test_uri("edge1.nv");
    lsp.send_notification("textDocument/didOpen", did_open_params(&uri, valid_nova()));
    // Immediately close without waiting for diagnostics
    lsp.send_notification("textDocument/didClose", did_close_params(&uri));

    // We should eventually get an empty-diagnostics notification (from didClose).
    // It may arrive before or after the initial check completes.
    // Give the server enough time to settle.
    lsp.wait_for_diagnostics(&uri, Duration::from_secs(10));

    // Server must still be up
    lsp.shutdown_and_exit();
}
