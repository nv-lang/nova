//! LSP lifecycle tests for nova-lsp — Plan 104.0.2.
//!
//! Spawns the real nova-lsp binary and communicates via JSON-RPC over stdio
//! (the actual LSP transport — Content-Length framing + JSON body).
//!
//! Tests:
//! - pos1: `initialize` response contains ServerCapabilities with
//!         textDocumentSync == Full (kind 1) and positionEncoding == "utf-16"
//! - pos2: full lifecycle initialize → initialized → shutdown → exit
//!         exits with code 0
//! - neg1: duplicate `initialize` returns InvalidRequest (-32600)

use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};

// ─────────────────────────────────────────────────────────────────────────────
// JSON-RPC over stdio helper
// ─────────────────────────────────────────────────────────────────────────────

/// Wraps a spawned nova-lsp process with send/receive helpers.
///
/// `stdin` is `Option` so it can be `take()`n when we want to close the
/// write end without moving `self` out of the struct (required because
/// `LspProcess` implements `Drop`).
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
            // Suppress tracing output so test output isn't cluttered.
            // Use `Stdio::inherit()` temporarily when debugging test failures.
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

    // ── Write side ──────────────────────────────────────────────────────────

    fn write_message(&mut self, msg: &serde_json::Value) {
        let stdin = self.stdin.as_mut().expect("stdin already closed");
        let body = msg.to_string();
        write!(stdin, "Content-Length: {}\r\n\r\n", body.len())
            .expect("write Content-Length header");
        stdin.write_all(body.as_bytes()).expect("write body");
        stdin.flush().expect("flush stdin");
    }

    /// Send a request (has `id`); returns the id assigned.
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

    /// Send a notification (no `id`; no response expected).
    fn send_notification(&mut self, method: &str, params: serde_json::Value) {
        self.write_message(&serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        }));
    }

    // ── Read side ────────────────────────────────────────────────────────────

    /// Read one JSON-RPC message from stdout (blocks until available).
    ///
    /// Parses the `Content-Length: N\r\n\r\n<body>` framing defined in
    /// the LSP base protocol (§Base Protocol Header Part).
    fn read_response(&mut self) -> serde_json::Value {
        let mut content_length: usize = 0;

        // Read header lines until blank line (signals end of headers).
        loop {
            let mut line = String::new();
            self.reader
                .read_line(&mut line)
                .expect("read header line from nova-lsp");
            assert!(!line.is_empty(), "nova-lsp closed stdout unexpectedly");

            if line == "\r\n" || line == "\n" {
                break; // end of headers
            }
            if let Some(rest) = line.strip_prefix("Content-Length:") {
                content_length = rest
                    .trim()
                    .parse()
                    .unwrap_or_else(|e| panic!("bad Content-Length value: {e}"));
            }
            // Ignore unknown headers (Content-Type, etc.) per LSP spec.
        }

        assert!(
            content_length > 0,
            "nova-lsp sent a response with Content-Length: 0"
        );

        let mut body = vec![0u8; content_length];
        self.reader
            .read_exact(&mut body)
            .unwrap_or_else(|e| panic!("failed to read {content_length}-byte body: {e}"));

        serde_json::from_slice(&body).unwrap_or_else(|e| {
            panic!(
                "invalid JSON in nova-lsp response: {e}\nbody: {}",
                String::from_utf8_lossy(&body)
            )
        })
    }

    // ── High-level LSP round-trips ───────────────────────────────────────────

    /// Read messages until we get one with `id == expected_id` (skips notifications).
    fn read_response_id(&mut self, expected_id: u64) -> serde_json::Value {
        loop {
            let msg = self.read_response();
            if msg.get("id").and_then(|v| v.as_u64()) == Some(expected_id) {
                return msg;
            }
            // Skip over server-initiated notifications (e.g., publishDiagnostics).
        }
    }

    /// Send a request with params and read the matching response (skips notifications).
    fn request(&mut self, method: &str, params: serde_json::Value) -> serde_json::Value {
        let id = self.send_request(method, params);
        self.read_response_id(id)
    }

    /// Send a request **without** a `params` field and read the response.
    ///
    /// Some LSP methods (e.g., `shutdown`) declare `params: void`.  tower-lsp
    /// validates strictly: sending `"params": null` produces -32602
    /// "Unexpected params".  Omitting the field entirely is correct.
    fn request_no_params(&mut self, method: &str) -> serde_json::Value {
        let id = self.next_id;
        self.next_id += 1;
        self.write_message(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
        }));
        self.read_response_id(id)
    }

    /// Send a minimal but spec-compliant `initialize` request.
    fn initialize(&mut self) -> serde_json::Value {
        self.request(
            "initialize",
            serde_json::json!({
                "processId": null,
                "rootUri": null,
                "capabilities": {
                    "general": {
                        "positionEncodings": ["utf-16"]
                    }
                },
            }),
        )
    }

    /// Send `initialized` notification (no response expected per LSP spec).
    fn initialized(&mut self) {
        self.send_notification("initialized", serde_json::json!({}));
    }

    /// Send `shutdown` request; return the response.
    ///
    /// LSP spec §3.5: `shutdown` has `params: void` → omit `params` field.
    fn shutdown(&mut self) -> serde_json::Value {
        self.request_no_params("shutdown")
    }

    /// Send `exit` notification, close stdin, and wait for the process to exit.
    ///
    /// Closing stdin is belt-and-suspenders: tower-lsp may call
    /// `std::process::exit()` on `exit`, or may let the serve loop return
    /// naturally on connection close.  Either way the process exits.
    fn exit_and_wait(mut self, timeout: Duration) -> std::process::ExitStatus {
        self.send_notification("exit", serde_json::Value::Null);
        drop(self.stdin.take()); // close stdin → EOF on server side

        let deadline = Instant::now() + timeout;
        loop {
            match self.child.try_wait().expect("child wait failed") {
                Some(status) => return status,
                None => {
                    if Instant::now() >= deadline {
                        let _ = self.child.kill();
                        panic!(
                            "nova-lsp did not exit within {:?} after 'exit' notification",
                            timeout
                        );
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
            }
        }
    }
}

impl Drop for LspProcess {
    /// Best-effort cleanup so the OS doesn't accumulate zombie processes when
    /// a test panics before the natural exit.
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

/// pos1: `initialize` returns capabilities with textDocumentSync == Full.
///
/// Verifies the full InitializeResult shape:
/// - `jsonrpc` == "2.0"
/// - No `error` field (success path)
/// - `result.serverInfo.name` == "nova-lsp"
/// - `result.capabilities.positionEncoding` == "utf-16"
/// - `result.capabilities.textDocumentSync.change` == 2  (Incremental, Plan 104.1.Ф.4)
#[test]
fn pos1_initialize_returns_incremental_sync_capabilities() {
    let mut lsp = LspProcess::spawn();
    let resp = lsp.initialize();

    // Basic JSON-RPC structure
    assert_eq!(resp["jsonrpc"], "2.0", "unexpected jsonrpc version: {resp}");
    assert!(
        resp.get("error").is_none(),
        "initialize returned an error: {resp}"
    );

    let result = &resp["result"];

    // Server identity
    assert_eq!(
        result["serverInfo"]["name"], "nova-lsp",
        "unexpected serverInfo.name: {result}"
    );

    // Position encoding: "utf-16" (LSP 3.17 string constant)
    assert_eq!(
        result["capabilities"]["positionEncoding"],
        "utf-16",
        "expected positionEncoding == utf-16, got: {}",
        result["capabilities"]["positionEncoding"]
    );

    // Plan 104.1.Ф.4: textDocumentSync is now an Options object with
    // change == 2 (Incremental, TextDocumentSyncKind enum in LSP spec).
    let sync = &result["capabilities"]["textDocumentSync"];
    let change_kind = &sync["change"];
    assert_eq!(
        *change_kind,
        serde_json::json!(2),
        "expected textDocumentSync.change == 2 (Incremental), got: {sync}"
    );
}

/// pos2: Full lifecycle — initialize → initialized → shutdown → exit — exits 0.
///
/// The LSP lifecycle requires this exact sequence. Verifies:
/// 1. `initialize` succeeds (no error).
/// 2. `initialized` notification accepted without crashing.
/// 3. `shutdown` returns JSON `null` (per LSP spec: "result: void").
/// 4. `exit` notification causes the process to exit with code 0.
#[test]
fn pos2_full_lifecycle_clean_exit() {
    let mut lsp = LspProcess::spawn();

    // 1. initialize
    let init_resp = lsp.initialize();
    assert!(
        init_resp.get("error").is_none(),
        "initialize failed: {init_resp}"
    );

    // 2. initialized notification — no response; server should stay alive
    lsp.initialized();

    // 3. shutdown — result MUST be null per LSP spec §3.5
    let shutdown_resp = lsp.shutdown();
    assert!(
        shutdown_resp.get("error").is_none(),
        "shutdown returned error: {shutdown_resp}"
    );
    assert_eq!(
        shutdown_resp["result"],
        serde_json::Value::Null,
        "shutdown result should be JSON null per LSP spec, got: {}",
        shutdown_resp["result"]
    );

    // 4. exit → process should exit cleanly with code 0
    let status = lsp.exit_and_wait(Duration::from_secs(10));
    assert!(
        status.success(),
        "nova-lsp exited non-zero after clean shutdown/exit: {status}"
    );
}

/// neg1: A second `initialize` request returns `InvalidRequest` (-32600).
///
/// LSP spec §3.15.1: calling `initialize` after it has already succeeded is
/// a protocol error.  tower-lsp's middleware intercepts this before reaching
/// our handler and returns `InvalidRequest`.
///
/// After receiving the error, the server must still be alive and accept a
/// clean `shutdown` → `exit` cycle (it must not have crashed or hung).
#[test]
fn neg1_duplicate_initialize_returns_invalid_request() {
    let mut lsp = LspProcess::spawn();

    // First initialize — must succeed
    let first_resp = lsp.initialize();
    assert!(
        first_resp.get("error").is_none(),
        "first initialize failed unexpectedly: {first_resp}"
    );

    // Second initialize — must return error -32600
    let second_resp = lsp.initialize();
    assert!(
        second_resp.get("error").is_some(),
        "expected an error on duplicate initialize, got success: {second_resp}"
    );

    let code = second_resp["error"]["code"]
        .as_i64()
        .unwrap_or_else(|| panic!("error.code is not an integer: {second_resp}"));
    assert_eq!(
        code, -32600,
        "expected InvalidRequest (-32600) on duplicate initialize, got {code}: {second_resp}"
    );

    // Server must still be alive — send a clean shutdown to verify
    let shutdown_resp = lsp.shutdown();
    assert!(
        shutdown_resp.get("error").is_none(),
        "server crashed after duplicate-init error; shutdown failed: {shutdown_resp}"
    );

    // Exit cleanly
    let status = lsp.exit_and_wait(Duration::from_secs(10));
    assert!(
        status.success(),
        "nova-lsp exited non-zero after duplicate-init test: {status}"
    );
}
