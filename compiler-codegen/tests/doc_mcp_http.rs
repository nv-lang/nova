//! Plan 45 Ф.34.1 — HTTP MCP transport integration tests.
//!
//! Spawns server в background thread, sends real HTTP POST requests,
//! verifies JSON-RPC responses. Uses ephemeral ports (0 = OS-assigned).

use nova_codegen::doc;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

fn make_tree_json() -> doc::json_parse::JsonValue {
    let json_str = r#"{
        "format_version": 1,
        "nova_version": "0.1.0",
        "modules": [{"path": "mymod"}],
        "items": [
            {"id": "mymod::greet", "name": "greet", "module_path": "mymod",
             "kind": "fn", "summary": "Says hello."}
        ]
    }"#;
    doc::json_parse::parse(json_str).expect("parse")
}

/// Pick ephemeral port by binding на :0, retrieving assigned port, dropping listener.
fn pick_port() -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = l.local_addr().unwrap().port();
    drop(l);
    // Brief sleep чтобы OS освободил порт.
    thread::sleep(Duration::from_millis(50));
    port
}

/// Send single POST /mcp request, read full response body. Closes connection.
fn http_post(port: u16, body: &str) -> Result<(u16, String), std::io::Error> {
    let mut stream = TcpStream::connect(("127.0.0.1", port))?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    let request = format!(
        "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{}",
        body.len(), body
    );
    stream.write_all(request.as_bytes())?;
    stream.flush()?;
    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    // Parse status line.
    let first_line = response.lines().next().unwrap_or("");
    let status: u16 = first_line.split_whitespace().nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    // Body — после `\r\n\r\n`.
    let body_start = response.find("\r\n\r\n").map(|i| i + 4).unwrap_or(response.len());
    let body_str = response[body_start..].to_string();
    Ok((status, body_str))
}

#[test]
fn http_server_responds_to_initialize() {
    let port = pick_port();
    let tree = Arc::new(make_tree_json());
    let tree_clone = Arc::clone(&tree);
    // Spawn server в detached thread.
    thread::spawn(move || {
        let _ = doc::mcp::run_http_server(&tree_clone, port);
    });
    // Give server time to bind.
    thread::sleep(Duration::from_millis(200));

    let (status, body) = http_post(port,
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#).expect("http");
    assert_eq!(status, 200);
    assert!(body.contains("protocolVersion"), "body должна содержать protocolVersion, got: {}", body);
    assert!(body.contains("nova-doc-mcp"));
}

#[test]
fn http_server_responds_to_tools_call() {
    let port = pick_port();
    let tree = Arc::new(make_tree_json());
    let tree_clone = Arc::clone(&tree);
    thread::spawn(move || {
        let _ = doc::mcp::run_http_server(&tree_clone, port);
    });
    thread::sleep(Duration::from_millis(200));

    let (status, body) = http_post(port,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"list_modules"}}"#).expect("http");
    assert_eq!(status, 200);
    assert!(body.contains("mymod"), "should mention module, got: {}", body);
}

#[test]
fn http_server_404_for_wrong_path() {
    let port = pick_port();
    let tree = Arc::new(make_tree_json());
    let tree_clone = Arc::clone(&tree);
    thread::spawn(move || {
        let _ = doc::mcp::run_http_server(&tree_clone, port);
    });
    thread::sleep(Duration::from_millis(200));

    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    stream.write_all(b"GET /other HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n").unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    assert!(response.starts_with("HTTP/1.1 404"), "expected 404, got: {}", response.lines().next().unwrap_or(""));
}

#[test]
fn http_server_400_for_bad_request_line() {
    let port = pick_port();
    let tree = Arc::new(make_tree_json());
    let tree_clone = Arc::clone(&tree);
    thread::spawn(move || {
        let _ = doc::mcp::run_http_server(&tree_clone, port);
    });
    thread::sleep(Duration::from_millis(200));

    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    stream.write_all(b"BADREQUEST\r\n\r\n").unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    assert!(response.starts_with("HTTP/1.1 400"), "expected 400, got: {}", response.lines().next().unwrap_or(""));
}
