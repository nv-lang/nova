//! Plan 45 Ф.32.3 — MCP server skeleton (JSON-RPC 2.0 over stdio).
//!
//! Minimal MCP-compatible JSON-RPC server. Реализует subset MCP spec:
//! - `initialize` — handshake
//! - `tools/list` — list available tools
//! - `tools/call` — execute tool с arguments
//!
//! **Tools exposed:**
//! - `query_items` — search items via Plan 45 Ф.32.1 query DSL
//! - `list_modules` — return all module paths
//! - `get_item` — fetch full item by id (raw JsonValue)
//!
//! **Protocol:** line-delimited JSON-RPC 2.0 messages на stdin/stdout.
//! Каждый message — один line valid JSON. Server reads stdin, processes,
//! writes response. Terminates когда stdin closes.
//!
//! **Not implemented (MVP scope):**
//! - SSE/HTTP transport (только stdio)
//! - `resources/*`, `prompts/*` MCP methods
//! - Capabilities negotiation (returns minimal capabilities)
//! - Server-initiated notifications (request-response only)
//!
//! **Why minimal:** real-world MCP clients (Claude Code, MCP Inspector)
//! work с stdio + line-delimited JSON-RPC. SSE/HTTP — Plan 45.A round 3.

use super::json_parse::{parse as parse_json, JsonValue};
use super::query::{parse_query, execute_json, render_results_json};

/// Plan 45 Ф.32.3 — entry point: run MCP loop reading from `input`,
/// writing to `output`. Returns when input EOF.
///
/// `tree_json` — pre-loaded `nova doc --format json` output (parsed JsonValue).
/// Server holds reference и serves queries against это.
pub fn run_mcp_loop<R: std::io::BufRead, W: std::io::Write>(
    tree_json: &JsonValue,
    mut input: R,
    mut output: W,
) -> std::io::Result<()> {
    let mut line = String::new();
    loop {
        line.clear();
        let n = input.read_line(&mut line)?;
        if n == 0 { break; } // EOF
        let trimmed = line.trim();
        if trimmed.is_empty() { continue; }
        let response = handle_request(tree_json, trimmed);
        writeln!(output, "{}", response)?;
        output.flush()?;
    }
    Ok(())
}

/// Plan 45 Ф.34.1 — HTTP MCP server (std::net::TcpListener, no tokio).
///
/// Single-threaded blocking accept loop. POST /mcp принимает JSON-RPC request
/// в body, returns response в body. Все other paths → 404.
///
/// Не production-grade HTTP (no keep-alive, no chunked encoding) — но
/// sufficient для local MCP integration с tools like Claude Code, MCP Inspector
/// которые поддерживают HTTP transport как fallback.
///
/// Bind на 127.0.0.1:port — explicitly localhost-only (security: no external
/// access).
pub fn run_http_server(tree_json: &JsonValue, port: u16) -> std::io::Result<()> {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    let addr = format!("127.0.0.1:{}", port);
    let listener = TcpListener::bind(&addr)?;
    eprintln!("nova doc-mcp HTTP server: listening on http://{} (POST /mcp)", addr);
    for stream in listener.incoming() {
        let mut stream = match stream {
            Ok(s) => s,
            Err(e) => { eprintln!("accept error: {}", e); continue; }
        };
        // Read HTTP request (blocking).
        let response = match handle_http_request(&mut stream, tree_json) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("http handler error: {}", e);
                http_response(500, "Internal Server Error", "text/plain", e.to_string().as_bytes())
            }
        };
        // Write response.
        let _ = stream.write_all(&response);
        let _ = stream.flush();
        let _ = stream.shutdown(std::net::Shutdown::Both);
        // (Manual cleanup — drop'аем stream после shutdown.)
        let _ = BufReader::new(&stream); // silence unused import warning
        let _: &mut dyn BufRead = &mut BufReader::new(&stream);
        let _: &mut dyn Read = &mut (&stream);
        let _: &mut dyn Write = &mut (&stream);
    }
    Ok(())
}

fn handle_http_request(stream: &mut std::net::TcpStream, tree_json: &JsonValue) -> std::io::Result<Vec<u8>> {
    use std::io::{BufRead, BufReader, Read};
    let mut reader = BufReader::new(stream.try_clone()?);
    // Read request line.
    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;
    let request_line = request_line.trim_end();
    let parts: Vec<&str> = request_line.split_whitespace().collect();
    if parts.len() < 3 {
        return Ok(http_response(400, "Bad Request", "text/plain", b"Malformed request line"));
    }
    let method = parts[0];
    let path = parts[1];
    // Read headers, find Content-Length.
    let mut content_length: usize = 0;
    loop {
        let mut header = String::new();
        let n = reader.read_line(&mut header)?;
        if n == 0 { break; }
        let trimmed = header.trim_end();
        if trimmed.is_empty() { break; } // headers/body separator
        if let Some(val) = trimmed.strip_prefix("Content-Length:").or_else(|| trimmed.strip_prefix("content-length:")) {
            content_length = val.trim().parse().unwrap_or(0);
        }
    }
    if method != "POST" || path != "/mcp" {
        return Ok(http_response(404, "Not Found", "text/plain", b"Use POST /mcp"));
    }
    // Read body (Content-Length bytes).
    let mut body = vec![0u8; content_length];
    if content_length > 0 {
        reader.read_exact(&mut body)?;
    }
    let body_str = match std::str::from_utf8(&body) {
        Ok(s) => s,
        Err(_) => return Ok(http_response(400, "Bad Request", "text/plain", b"Body not UTF-8")),
    };
    // Process JSON-RPC.
    let response = handle_request(tree_json, body_str);
    Ok(http_response(200, "OK", "application/json", response.as_bytes()))
}

fn http_response(status_code: u16, status_text: &str, content_type: &str, body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(128 + body.len());
    out.extend_from_slice(format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        status_code, status_text, content_type, body.len()
    ).as_bytes());
    out.extend_from_slice(body);
    out
}

/// Plan 45 Ф.32.3 — public for testing: handle one JSON-RPC request line,
/// return response as JSON string.
pub fn handle_request(tree_json: &JsonValue, request_line: &str) -> String {
    let request = match parse_json(request_line) {
        Ok(v) => v,
        Err(e) => return jsonrpc_error(None, -32700, &format!("Parse error: {}", e)),
    };
    let id = request.get("id").cloned();
    let method = match request.get("method").and_then(|v| v.as_str()) {
        Some(m) => m.to_string(),
        None => return jsonrpc_error(id, -32600, "Invalid Request: missing method"),
    };
    let params = request.get("params");
    match method.as_str() {
        "initialize" => handle_initialize(id),
        "tools/list" => handle_tools_list(id),
        "tools/call" => handle_tools_call(tree_json, id, params),
        _ => jsonrpc_error(id, -32601, &format!("Method not found: {}", method)),
    }
}

fn handle_initialize(id: Option<JsonValue>) -> String {
    // Minimal MCP initialize response. Protocol version `2024-11-05` —
    // current stable per MCP spec.
    jsonrpc_result(id, &format!(
        "{{\"protocolVersion\":\"2024-11-05\",\"serverInfo\":{{\"name\":\"nova-doc-mcp\",\"version\":\"{}\"}},\"capabilities\":{{\"tools\":{{}}}}}}",
        env!("CARGO_PKG_VERSION")
    ))
}

fn handle_tools_list(id: Option<JsonValue>) -> String {
    // Compact one-line tools schema (no internal newlines — JSON-RPC line protocol).
    let tools = r#"[{"name":"query_items","description":"Search doc items via DSL (kind/name/module/capability/effect/has-contracts/verified/stability/deprecated).","inputSchema":{"type":"object","properties":{"query":{"type":"string"}},"required":["query"]}},{"name":"list_modules","description":"Return all module paths in the doc tree.","inputSchema":{"type":"object","properties":{}}},{"name":"get_item","description":"Fetch full item JSON by id.","inputSchema":{"type":"object","properties":{"item_id":{"type":"string"}},"required":["item_id"]}}]"#;
    jsonrpc_result(id, &format!("{{\"tools\":{}}}", tools))
}

fn handle_tools_call(
    tree_json: &JsonValue,
    id: Option<JsonValue>,
    params: Option<&JsonValue>,
) -> String {
    let params = match params {
        Some(p) => p,
        None => return jsonrpc_error(id, -32602, "Invalid params: missing"),
    };
    let name = match params.get("name").and_then(|v| v.as_str()) {
        Some(n) => n,
        None => return jsonrpc_error(id, -32602, "Invalid params: missing name"),
    };
    let arguments = params.get("arguments");
    let result_text = match name {
        "query_items" => tool_query_items(tree_json, arguments),
        "list_modules" => tool_list_modules(tree_json),
        "get_item" => tool_get_item(tree_json, arguments),
        other => return jsonrpc_error(id, -32602, &format!("Unknown tool: {}", other)),
    };
    match result_text {
        Ok(text) => jsonrpc_result(id, &format!(
            "{{\"content\":[{{\"type\":\"text\",\"text\":{}}}]}}",
            json_string(&text)
        )),
        Err(msg) => jsonrpc_error(id, -32603, &msg),
    }
}

fn tool_query_items(tree_json: &JsonValue, args: Option<&JsonValue>) -> Result<String, String> {
    let query_str = args.and_then(|a| a.get("query")).and_then(|v| v.as_str())
        .ok_or_else(|| "missing query argument".to_string())?;
    let q = parse_query(query_str).map_err(|e| format!("query parse: {}", e))?;
    let results = execute_json(tree_json, &q);
    Ok(render_results_json(&results))
}

fn tool_list_modules(tree_json: &JsonValue) -> Result<String, String> {
    let modules = tree_json.get("modules").and_then(|v| v.as_array());
    let mut paths: Vec<&str> = match modules {
        Some(arr) => arr.iter()
            .filter_map(|m| m.get("path").and_then(|v| v.as_str()))
            .collect(),
        None => Vec::new(),
    };
    paths.sort();
    let mut out = String::from("[");
    for (i, p) in paths.iter().enumerate() {
        if i > 0 { out.push_str(","); }
        out.push_str(&json_string(p));
    }
    out.push(']');
    Ok(out)
}

fn tool_get_item(tree_json: &JsonValue, args: Option<&JsonValue>) -> Result<String, String> {
    let id = args.and_then(|a| a.get("item_id")).and_then(|v| v.as_str())
        .ok_or_else(|| "missing item_id argument".to_string())?;
    let items = tree_json.get("items").and_then(|v| v.as_array())
        .ok_or_else(|| "no `items` array в JSON".to_string())?;
    for item in items {
        if item.get("id").and_then(|v| v.as_str()) == Some(id) {
            return Ok(json_value_to_string(item));
        }
    }
    Err(format!("item not found: {}", id))
}

/// Serialize JsonValue back to JSON string. Minimal serialization
/// (без pretty-print) — output для machine consumption.
fn json_value_to_string(v: &JsonValue) -> String {
    let mut out = String::with_capacity(64);
    write_json_value(&mut out, v);
    out
}

fn write_json_value(out: &mut String, v: &JsonValue) {
    use std::fmt::Write;
    match v {
        JsonValue::Null => out.push_str("null"),
        JsonValue::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        JsonValue::Int(n) => { let _ = write!(out, "{}", n); }
        JsonValue::Str(s) => out.push_str(&json_string(s)),
        JsonValue::Array(arr) => {
            out.push('[');
            for (i, e) in arr.iter().enumerate() {
                if i > 0 { out.push(','); }
                write_json_value(out, e);
            }
            out.push(']');
        }
        JsonValue::Object(map) => {
            out.push('{');
            for (i, (k, val)) in map.iter().enumerate() {
                if i > 0 { out.push(','); }
                out.push_str(&json_string(k));
                out.push(':');
                write_json_value(out, val);
            }
            out.push('}');
        }
    }
}

/// Escape Rust string as JSON string literal (с кавычками).
fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{0008}' => out.push_str("\\b"),
            '\u{000C}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => {
                use std::fmt::Write;
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn jsonrpc_result(id: Option<JsonValue>, result_json: &str) -> String {
    let id_str = id_as_string(id);
    format!("{{\"jsonrpc\":\"2.0\",\"id\":{},\"result\":{}}}", id_str, result_json)
}

fn jsonrpc_error(id: Option<JsonValue>, code: i32, message: &str) -> String {
    let id_str = id_as_string(id);
    format!(
        "{{\"jsonrpc\":\"2.0\",\"id\":{},\"error\":{{\"code\":{},\"message\":{}}}}}",
        id_str, code, json_string(message)
    )
}

fn id_as_string(id: Option<JsonValue>) -> String {
    match id {
        None => "null".to_string(),
        Some(JsonValue::Null) => "null".to_string(),
        Some(JsonValue::Int(n)) => n.to_string(),
        Some(JsonValue::Str(s)) => json_string(&s),
        Some(other) => json_value_to_string(&other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tree_json() -> JsonValue {
        let json_str = r#"{
            "format_version": 1,
            "nova_version": "0.1.0",
            "modules": [{"path": "m"}, {"path": "other"}],
            "items": [
                {"id": "m::add", "name": "add", "module_path": "m", "kind": "fn",
                 "summary": "Adds two ints.", "capabilities": {"pure_fn": true}},
                {"id": "m::sub", "name": "sub", "module_path": "m", "kind": "fn",
                 "summary": "Subtracts.", "capabilities": {"pure_fn": false}}
            ]
        }"#;
        parse_json(json_str).expect("parse")
    }

    #[test]
    fn initialize_returns_protocol_version() {
        let tree = make_tree_json();
        let req = r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#;
        let resp = handle_request(&tree, req);
        assert!(resp.contains("\"protocolVersion\":\"2024-11-05\""));
        assert!(resp.contains("\"serverInfo\""));
        assert!(resp.contains("nova-doc-mcp"));
    }

    #[test]
    fn tools_list_returns_three_tools() {
        let tree = make_tree_json();
        let req = r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#;
        let resp = handle_request(&tree, req);
        assert!(resp.contains("query_items"));
        assert!(resp.contains("list_modules"));
        assert!(resp.contains("get_item"));
    }

    #[test]
    fn tools_call_list_modules() {
        let tree = make_tree_json();
        let req = r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"list_modules"}}"#;
        let resp = handle_request(&tree, req);
        // Strings escaped в text content (двойное escape).
        assert!(resp.contains("\\\"m\\\""), "should contain escaped \\\"m\\\", got: {}", resp);
        assert!(resp.contains("\\\"other\\\""));
    }

    #[test]
    fn tools_call_query_items_kind() {
        let tree = make_tree_json();
        let req = r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"query_items","arguments":{"query":"kind=fn"}}}"#;
        let resp = handle_request(&tree, req);
        // Response: text content escaped → "add" может быть как \"add\" внутри JSON string.
        assert!(resp.contains("add"), "resp должна содержать `add`, got: {}", resp);
        assert!(resp.contains("sub"), "resp должна содержать `sub`, got: {}", resp);
    }

    #[test]
    fn tools_call_query_items_capability_pure() {
        let tree = make_tree_json();
        let req = r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"query_items","arguments":{"query":"capability=pure"}}}"#;
        let resp = handle_request(&tree, req);
        assert!(resp.contains("add"), "pure fn `add` должен match, got: {}", resp);
        // sub НЕ должно появиться в result text (но может быть в schema).
        // Проверим что result содержит add но не sub в кавычках "name".
        assert!(resp.contains("\\\"name\\\": \\\"add\\\""),
            "result должен содержать add как item name");
        assert!(!resp.contains("\\\"name\\\": \\\"sub\\\""),
            "non-pure sub НЕ должен быть в result");
    }

    #[test]
    fn tools_call_get_item_existing() {
        let tree = make_tree_json();
        let req = r#"{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"get_item","arguments":{"item_id":"m::add"}}}"#;
        let resp = handle_request(&tree, req);
        assert!(resp.contains("add"), "resp должна mention add");
        assert!(resp.contains("Adds two ints"));
    }

    #[test]
    fn tools_call_get_item_missing_errors() {
        let tree = make_tree_json();
        let req = r#"{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"get_item","arguments":{"item_id":"m::nonexistent"}}}"#;
        let resp = handle_request(&tree, req);
        assert!(resp.contains("\"error\""));
        assert!(resp.contains("item not found"));
    }

    #[test]
    fn unknown_method_errors() {
        let tree = make_tree_json();
        let req = r#"{"jsonrpc":"2.0","id":8,"method":"nonsense"}"#;
        let resp = handle_request(&tree, req);
        assert!(resp.contains("Method not found"));
        assert!(resp.contains("-32601"));
    }

    #[test]
    fn invalid_json_returns_parse_error() {
        let tree = make_tree_json();
        let resp = handle_request(&tree, "not valid json{");
        assert!(resp.contains("Parse error"));
        assert!(resp.contains("-32700"));
    }

    #[test]
    fn mcp_loop_processes_multiple_requests() {
        let tree = make_tree_json();
        let input = b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\"}\n{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\"}\n";
        let mut output = Vec::new();
        run_mcp_loop(&tree, &input[..], &mut output).expect("loop");
        let out_str = String::from_utf8(output).unwrap();
        let lines: Vec<&str> = out_str.lines().collect();
        assert_eq!(lines.len(), 2, "expected 2 responses for 2 requests, got {} lines", lines.len());
        assert!(lines[0].contains("protocolVersion"), "first response — initialize, got: {}", lines[0]);
        assert!(lines[1].contains("query_items"), "second response — tools/list, got: {}", lines[1]);
    }

    #[test]
    fn json_string_escapes_special_chars() {
        assert_eq!(json_string("hello"), "\"hello\"");
        assert_eq!(json_string("a\nb"), "\"a\\nb\"");
        assert_eq!(json_string("a\"b"), "\"a\\\"b\"");
        assert_eq!(json_string("a\\b"), "\"a\\\\b\"");
    }
}
