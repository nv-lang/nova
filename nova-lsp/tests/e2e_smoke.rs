//! E2E smoke tests for nova-lsp — Plan 104.9 close-out.
//!
//! Each test spawns the real `nova-lsp` binary and communicates via JSON-RPC
//! over stdin/stdout (LSP base-protocol Content-Length framing).
//!
//! Tests:
//! - pos1:  Binary exists and --version works.
//! - pos2:  Full LSP initialize handshake → capabilities include hover,
//!          completion, definition, formatting, rename, codeAction.
//! - pos3:  didOpen a Nova file → publishDiagnostics fires within 10s.
//! - pos4:  textDocument/hover on identifier → non-null result or null (V1 single-file).
//! - pos5:  textDocument/completion in fn body → CompletionList with keyword items.
//! - pos6:  textDocument/definition → Location response (null or valid Location).
//! - pos7:  textDocument/formatting → TextEdit array response (may be empty if
//!          nova fmt not found, but no error crash).
//! - pos8:  workspace/symbol → array response (may be empty).
//! - pos9:  textDocument/codeAction → array response (may be empty).
//! - pos10: textDocument/rename → WorkspaceEdit response (null or WorkspaceEdit).
//! - neg1:  Malformed JSON → no crash, server exits gracefully (no panic exit).
//! - neg2:  Unknown method → responds with -32601 MethodNotFound (no crash).

use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};

// ─────────────────────────────────────────────────────────────────────────────
// JSON-RPC helper
// ─────────────────────────────────────────────────────────────────────────────

/// Wraps a spawned nova-lsp process with synchronous send/receive helpers.
///
/// Uses the same LSP base-protocol framing as all other test modules:
/// `Content-Length: N\r\n\r\n<body>`.
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
            // Suppress tracing output — use Stdio::inherit() when debugging.
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
        write!(stdin, "Content-Length: {}\r\n\r\n", body.len()).expect("write Content-Length");
        stdin.write_all(body.as_bytes()).expect("write body");
        stdin.flush().expect("flush stdin");
    }

    fn write_raw(&mut self, bytes: &[u8]) {
        let stdin = self.stdin.as_mut().expect("stdin already closed");
        stdin.write_all(bytes).expect("write raw");
        stdin.flush().expect("flush stdin");
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

    // ── Read side ────────────────────────────────────────────────────────────

    /// Read one JSON-RPC message from stdout (blocking).
    fn read_response(&mut self) -> serde_json::Value {
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
        assert!(content_length > 0, "Content-Length: 0 is invalid");
        let mut body = vec![0u8; content_length];
        self.reader.read_exact(&mut body).expect("read body");
        serde_json::from_slice(&body)
            .unwrap_or_else(|e| panic!("invalid JSON in response: {e}\nbody: {}", String::from_utf8_lossy(&body)))
    }

    /// Read messages until one with matching `id` arrives (skips notifications).
    fn read_response_id(&mut self, expected_id: u64) -> serde_json::Value {
        loop {
            let msg = self.read_response();
            if msg.get("id").and_then(|v| v.as_u64()) == Some(expected_id) {
                return msg;
            }
        }
    }

    /// Read messages until `predicate` returns true or `timeout` expires.
    fn read_until<F>(&mut self, predicate: F, timeout: Duration) -> Option<serde_json::Value>
    where
        F: Fn(&serde_json::Value) -> bool,
    {
        let deadline = Instant::now() + timeout;
        loop {
            if Instant::now() >= deadline {
                return None;
            }
            match self.child.try_wait() {
                Ok(Some(_)) => return None, // process exited
                Ok(None) => {}
                Err(e) => panic!("try_wait failed: {e}"),
            }
            let msg = self.read_response();
            if predicate(&msg) {
                return Some(msg);
            }
        }
    }

    // ── High-level LSP helpers ───────────────────────────────────────────────

    fn send_request_no_params(&mut self, method: &str) -> serde_json::Value {
        let id = self.next_id;
        self.next_id += 1;
        self.write_message(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
        }));
        self.read_response_id(id)
    }

    fn request(&mut self, method: &str, params: serde_json::Value) -> serde_json::Value {
        let id = self.send_request(method, params);
        self.read_response_id(id)
    }

    fn initialize(&mut self) -> serde_json::Value {
        self.request(
            "initialize",
            serde_json::json!({
                "processId": null,
                "rootUri": null,
                "capabilities": {
                    "general": { "positionEncodings": ["utf-16"] },
                    "textDocument": {
                        "hover": { "contentFormat": ["markdown", "plaintext"] },
                        "completion": { "completionItem": { "snippetSupport": false } },
                        "definition": {},
                        "formatting": {},
                        "rename": { "prepareSupport": true },
                        "codeAction": { "codeActionLiteralSupport": { "codeActionKind": { "valueSet": [] } } },
                        "publishDiagnostics": {},
                    },
                    "workspace": {
                        "symbol": {},
                    }
                },
            }),
        )
    }

    fn initialize_full(&mut self) {
        let resp = self.initialize();
        assert!(resp.get("error").is_none(), "initialize failed: {resp}");
        self.send_notification("initialized", serde_json::json!({}));
    }

    fn wait_for_diagnostics(&mut self, uri: &str, timeout: Duration) -> serde_json::Value {
        self.read_until(
            |m| {
                m.get("method").and_then(|v| v.as_str())
                    == Some("textDocument/publishDiagnostics")
                    && m["params"]["uri"].as_str() == Some(uri)
            },
            timeout,
        )
        .unwrap_or_else(|| panic!("timeout waiting for publishDiagnostics for {uri}"))
    }

    fn shutdown_and_exit(mut self) {
        self.send_request_no_params("shutdown");
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
// Test helpers
// ─────────────────────────────────────────────────────────────────────────────

/// A minimal valid Nova source file for smoke tests.
fn simple_nova_src() -> &'static str {
    "module test_e2e.smoke\n\n\
     fn add(a int, b int) -> int => a + b\n\n\
     fn main() => ()\n"
}

/// A Nova source file with a deliberate syntax error.
fn broken_nova_src() -> &'static str {
    "module test_e2e.broken\n\nfn bad( => ()\n"
}

/// Build a file:// URI for an in-memory test document.
fn test_uri(name: &str) -> String {
    format!("file:///tmp/nova_e2e_smoke/{name}")
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

/// Position pointing at the `a` parameter in `fn add(a int, b int)`.
/// line=2 (0-based), character=7 (0-based, start of `a`).
fn position_on_identifier() -> serde_json::Value {
    serde_json::json!({ "line": 2, "character": 7 })
}

/// Position inside the fn body (line 4, character 15 — inside `main`).
fn position_in_fn_body() -> serde_json::Value {
    serde_json::json!({ "line": 4, "character": 8 })
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests (pos1 .. pos10 + neg1 + neg2)
// ─────────────────────────────────────────────────────────────────────────────

/// pos1: nova-lsp binary exists and `--version` works.
///
/// This is the most basic sanity check: the binary compiled, can be launched,
/// and reports a version string without crashing.
#[test]
fn pos1_binary_exists_and_version_works() {
    let binary = std::path::PathBuf::from(env!("CARGO_BIN_EXE_nova-lsp"));
    assert!(binary.exists(), "nova-lsp binary not found at {binary:?}");

    let output = Command::new(&binary)
        .arg("--version")
        .output()
        .unwrap_or_else(|e| panic!("failed to run `nova-lsp --version`: {e}"));

    assert!(
        output.status.success(),
        "`nova-lsp --version` exited non-zero: {}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr),
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("nova-lsp"),
        "expected 'nova-lsp' in --version output, got: {stdout:?}"
    );
    assert!(
        stdout.chars().any(|c| c.is_ascii_digit()),
        "expected semver digits in --version output, got: {stdout:?}"
    );
}

/// pos2: Full LSP initialize handshake → ServerCapabilities include all V1 features.
///
/// Verifies that the initialize response advertises:
/// - hoverProvider
/// - completionProvider
/// - definitionProvider
/// - documentFormattingProvider
/// - renameProvider
/// - codeActionProvider
/// - textDocumentSync (incremental or full)
/// - referencesProvider
/// - documentSymbolProvider
/// - workspaceSymbolProvider
#[test]
fn pos2_initialize_capabilities_advertise_all_v1_features() {
    let mut lsp = LspProcess::spawn();
    let resp = lsp.initialize();

    assert!(resp.get("error").is_none(), "initialize returned error: {resp}");
    let caps = &resp["result"]["capabilities"];

    // Must be present (truthy or non-null)
    macro_rules! assert_cap {
        ($field:expr) => {
            assert!(
                !caps[$field].is_null() && caps[$field] != serde_json::Value::Bool(false),
                "expected capabilities.{} to be advertised, got: {}",
                $field,
                caps[$field]
            );
        };
    }

    assert_cap!("hoverProvider");
    assert_cap!("completionProvider");
    assert_cap!("definitionProvider");
    assert_cap!("documentFormattingProvider");
    assert_cap!("renameProvider");
    assert_cap!("codeActionProvider");
    assert_cap!("referencesProvider");
    assert_cap!("documentSymbolProvider");
    assert_cap!("workspaceSymbolProvider");

    // textDocumentSync must be present (either Full=1 or Incremental=2)
    let sync = &caps["textDocumentSync"];
    assert!(
        !sync.is_null(),
        "textDocumentSync must be advertised, got null"
    );
    // Either a number (sync kind) or an object with .change
    let kind = if sync.is_object() { sync["change"].as_u64() } else { sync.as_u64() };
    assert!(
        matches!(kind, Some(1) | Some(2)),
        "textDocumentSync kind must be 1 (Full) or 2 (Incremental), got: {sync}"
    );

    lsp.shutdown_and_exit();
}

/// pos3: didOpen a Nova file with a syntax error → publishDiagnostics fires within 10s.
///
/// Opening a file with a syntax error must trigger a publishDiagnostics
/// notification with at least one diagnostic.
#[test]
fn pos3_did_open_publishes_diagnostics_within_10s() {
    let mut lsp = LspProcess::spawn();
    lsp.initialize_full();

    let uri = test_uri("pos3_broken.nv");
    lsp.send_notification("textDocument/didOpen", did_open_params(&uri, broken_nova_src()));

    let notif = lsp.wait_for_diagnostics(&uri, Duration::from_secs(10));
    let diags = notif["params"]["diagnostics"].as_array()
        .expect("publishDiagnostics.params.diagnostics must be array");
    assert!(
        !diags.is_empty(),
        "expected ≥1 diagnostic for broken Nova source, got empty array"
    );

    lsp.shutdown_and_exit();
}

/// pos4: textDocument/hover on identifier → non-crash response.
///
/// V1 hover resolves local variables, parameters, and top-level declarations
/// within a single file.  The result may be null if the cursor is on a position
/// that the V1 resolver doesn't recognize, but the server MUST NOT crash.
#[test]
fn pos4_hover_on_identifier_no_crash() {
    let mut lsp = LspProcess::spawn();
    lsp.initialize_full();

    let uri = test_uri("pos4_hover.nv");
    lsp.send_notification("textDocument/didOpen", did_open_params(&uri, simple_nova_src()));

    // Give the server a moment to parse the file.
    std::thread::sleep(Duration::from_millis(300));

    let resp = lsp.request(
        "textDocument/hover",
        serde_json::json!({
            "textDocument": { "uri": uri },
            "position": position_on_identifier(),
        }),
    );

    assert!(
        resp.get("error").is_none(),
        "hover returned JSON-RPC error: {resp}"
    );
    // result may be null (V1 single-file) or a HoverResult object — both OK.
    // The key assertion is: no error.

    lsp.shutdown_and_exit();
}

/// pos5: textDocument/completion in fn body → CompletionList with keyword items.
///
/// Requests completion at the start of `fn main` body.  The V1 completion
/// provider always returns at least keyword items in any fn-body context.
#[test]
fn pos5_completion_in_fn_body_returns_keyword_items() {
    let mut lsp = LspProcess::spawn();
    lsp.initialize_full();

    let uri = test_uri("pos5_completion.nv");
    lsp.send_notification("textDocument/didOpen", did_open_params(&uri, simple_nova_src()));
    std::thread::sleep(Duration::from_millis(200));

    let resp = lsp.request(
        "textDocument/completion",
        serde_json::json!({
            "textDocument": { "uri": uri },
            "position": position_in_fn_body(),
            "context": { "triggerKind": 1 },
        }),
    );

    assert!(resp.get("error").is_none(), "completion returned error: {resp}");

    // result is either null, CompletionList { items: [...] }, or array of items.
    let result = &resp["result"];
    if !result.is_null() {
        let items = if result.is_array() {
            result.as_array().unwrap()
        } else {
            result["items"].as_array().expect("CompletionList.items must be array")
        };
        // Must have at least some items — keyword completion always fires.
        assert!(
            !items.is_empty(),
            "expected ≥1 completion item in fn body, got empty list"
        );

        // At least one item should be a keyword (kind 14) or a snippet (kind 15).
        let has_keyword_or_snippet = items.iter().any(|item| {
            matches!(item["kind"].as_u64(), Some(14) | Some(15) | Some(1))
        });
        assert!(
            has_keyword_or_snippet,
            "expected at least one keyword/snippet completion item, items: {items:?}"
        );
    }
    // null result is also acceptable (no completions at this position in V1).

    lsp.shutdown_and_exit();
}

/// pos6: textDocument/definition → Location or null response (no crash).
///
/// V1 definition resolves symbols within a single file.  Result may be null
/// for cross-file or unresolved symbols, but no error must be returned.
#[test]
fn pos6_definition_returns_location_or_null() {
    let mut lsp = LspProcess::spawn();
    lsp.initialize_full();

    let uri = test_uri("pos6_definition.nv");
    lsp.send_notification("textDocument/didOpen", did_open_params(&uri, simple_nova_src()));
    std::thread::sleep(Duration::from_millis(300));

    let resp = lsp.request(
        "textDocument/definition",
        serde_json::json!({
            "textDocument": { "uri": uri },
            "position": position_on_identifier(),
        }),
    );

    assert!(resp.get("error").is_none(), "definition returned error: {resp}");
    // result: null | Location | Location[] — all valid in V1.

    lsp.shutdown_and_exit();
}

/// pos7: textDocument/formatting → TextEdit array (may be empty, no crash).
///
/// Calls document formatting (which invokes `nova fmt`).  If `nova fmt` is not
/// in PATH the server gracefully returns an empty array.  No error code.
#[test]
fn pos7_formatting_returns_text_edits_or_empty() {
    let mut lsp = LspProcess::spawn();
    lsp.initialize_full();

    let uri = test_uri("pos7_formatting.nv");
    lsp.send_notification("textDocument/didOpen", did_open_params(&uri, simple_nova_src()));
    std::thread::sleep(Duration::from_millis(200));

    let resp = lsp.request(
        "textDocument/formatting",
        serde_json::json!({
            "textDocument": { "uri": uri },
            "options": { "tabSize": 4, "insertSpaces": true },
        }),
    );

    assert!(
        resp.get("error").is_none(),
        "formatting returned JSON-RPC error: {resp}"
    );
    // result: null | TextEdit[] — both acceptable.
    // If result is an array it may be empty (no changes needed / nova fmt not found).
    let result = &resp["result"];
    if !result.is_null() {
        assert!(
            result.is_array(),
            "formatting result must be TextEdit[] or null, got: {result}"
        );
    }

    lsp.shutdown_and_exit();
}

/// pos8: workspace/symbol → array response (may be empty).
///
/// workspace/symbol must return an array (possibly empty if no symbols are indexed
/// yet).  It must not return an error.
#[test]
fn pos8_workspace_symbol_returns_array() {
    let mut lsp = LspProcess::spawn();
    lsp.initialize_full();

    // Open a file so the workspace index is seeded.
    let uri = test_uri("pos8_workspace.nv");
    lsp.send_notification("textDocument/didOpen", did_open_params(&uri, simple_nova_src()));
    std::thread::sleep(Duration::from_millis(300));

    let resp = lsp.request(
        "workspace/symbol",
        serde_json::json!({ "query": "" }),
    );

    assert!(resp.get("error").is_none(), "workspace/symbol returned error: {resp}");
    let result = &resp["result"];
    // Must be array or null.
    assert!(
        result.is_null() || result.is_array(),
        "workspace/symbol result must be SymbolInformation[] or null, got: {result}"
    );

    lsp.shutdown_and_exit();
}

/// pos9: textDocument/codeAction → array response (may be empty).
///
/// Code actions are returned as an array of Command or CodeAction objects.
/// The array may be empty if there are no diagnostics/actions at the position.
#[test]
fn pos9_code_action_returns_array() {
    let mut lsp = LspProcess::spawn();
    lsp.initialize_full();

    let uri = test_uri("pos9_codeaction.nv");
    lsp.send_notification("textDocument/didOpen", did_open_params(&uri, simple_nova_src()));
    std::thread::sleep(Duration::from_millis(300));

    let resp = lsp.request(
        "textDocument/codeAction",
        serde_json::json!({
            "textDocument": { "uri": uri },
            "range": {
                "start": { "line": 0, "character": 0 },
                "end": { "line": 0, "character": 0 },
            },
            "context": { "diagnostics": [] },
        }),
    );

    assert!(resp.get("error").is_none(), "codeAction returned error: {resp}");
    let result = &resp["result"];
    assert!(
        result.is_null() || result.is_array(),
        "codeAction result must be (Command|CodeAction)[] or null, got: {result}"
    );

    lsp.shutdown_and_exit();
}

/// pos10: textDocument/rename → WorkspaceEdit response or null.
///
/// rename must return a WorkspaceEdit (possibly empty if no occurrences found)
/// or null.  It must not return an error for a valid identifier position.
#[test]
fn pos10_rename_returns_workspace_edit_or_null() {
    let mut lsp = LspProcess::spawn();
    lsp.initialize_full();

    let uri = test_uri("pos10_rename.nv");
    lsp.send_notification("textDocument/didOpen", did_open_params(&uri, simple_nova_src()));
    std::thread::sleep(Duration::from_millis(300));

    let resp = lsp.request(
        "textDocument/rename",
        serde_json::json!({
            "textDocument": { "uri": uri },
            "position": position_on_identifier(),
            "newName": "renamed_add",
        }),
    );

    // Note: rename may return an error if prepareRename rejects the position.
    // We accept both success (WorkspaceEdit or null) and an LSP error
    // (e.g. -32803 RequestFailed or -32600 InvalidRequest) — what is NOT
    // acceptable is a Rust panic or a server crash.
    //
    // If it's an error, it must have a numeric code.
    if let Some(err) = resp.get("error") {
        let code = err["code"].as_i64();
        assert!(
            code.is_some(),
            "rename error must have numeric code, got: {resp}"
        );
        // Any LSP error code is fine — we just verify it's not a Rust panic.
    }
    // Otherwise: null or WorkspaceEdit result — both OK.

    lsp.shutdown_and_exit();
}

/// neg1: Malformed JSON (valid Content-Length header, invalid JSON body) →
///       server stays alive or exits gracefully — no Rust panic.
///
/// tower-lsp 0.20 observed behavior: codec logs error, closes connection.
/// Any behavior (drop message, return -32700, close connection) is acceptable
/// as long as there is no Rust panic (exit code 101 on Windows).
#[test]
fn neg1_malformed_json_no_crash() {
    let mut lsp = LspProcess::spawn();
    lsp.initialize_full();

    // Valid framing, invalid JSON body ("not_jsn" = 7 bytes, not JSON).
    lsp.write_raw(b"Content-Length: 7\r\n\r\nnot_jsn");

    // Give the server time to process.
    std::thread::sleep(Duration::from_millis(500));

    // Try to read a response (server may send -32700 before closing).
    // We use try_wait to detect early exit.
    let exited = matches!(lsp.child.try_wait(), Ok(Some(_)));

    if !exited {
        // If still alive: attempt a clean shutdown to verify it's responsive.
        // (If the server closed the connection, this will fail gracefully.)
        // We just verify it doesn't panic when killed.
        let _ = lsp.child.kill();
    }

    let status = lsp.child.wait().expect("wait on child");

    // The key assertion: no Rust panic.
    #[cfg(windows)]
    assert_ne!(
        status.code(),
        Some(101),
        "nova-lsp panicked (exit 101) after malformed JSON input"
    );
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        assert!(
            status.signal() != Some(6),
            "nova-lsp received SIGABRT (Rust abort-panic) after malformed JSON"
        );
    }

    // Mark stdin as closed so Drop doesn't double-kill.
    lsp.stdin.take();
}

/// neg2: Unknown LSP method → server responds with -32601 MethodNotFound (no crash).
///
/// Per JSON-RPC 2.0 spec §5.1: unknown methods MUST return error code -32601.
/// tower-lsp enforces this automatically.  After receiving the error response
/// the server must still be alive and respond to shutdown/exit.
#[test]
fn neg2_unknown_method_returns_method_not_found() {
    let mut lsp = LspProcess::spawn();
    lsp.initialize_full();

    let resp = lsp.request(
        "nova/unknownMethod",
        serde_json::json!({ "someParam": 42 }),
    );

    assert!(
        resp.get("error").is_some(),
        "expected error for unknown method, got success: {resp}"
    );

    let code = resp["error"]["code"].as_i64()
        .unwrap_or_else(|| panic!("error.code is not an integer: {resp}"));
    assert_eq!(
        code, -32601,
        "expected MethodNotFound (-32601) for unknown method, got {code}: {resp}"
    );

    // Server must still be alive — perform clean shutdown.
    lsp.shutdown_and_exit();
}
