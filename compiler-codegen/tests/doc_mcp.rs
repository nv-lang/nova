//! Plan 45 Ф.32.3 — MCP server integration tests.

use nova_codegen::doc;
use nova_codegen::doc::mcp::{handle_request, run_mcp_loop};
use nova_codegen::parser;
use nova_codegen::types;

fn tree_json_for(src: &str) -> doc::json_parse::JsonValue {
    let mut module = parser::parse(src).expect("parse");
    let _ = types::check_module(&module);
    types::infer_effects(&mut module);
    let tree = doc::build(&module);
    let json_str = doc::render_json(&tree);
    doc::json_parse::parse(&json_str).expect("re-parse JSON")
}

#[test]
fn mcp_initialize_response_well_formed() {
    let tree = tree_json_for("module m\nexport fn f() -> int => 1\n");
    let resp = handle_request(&tree, r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#);
    // Parsed back as JSON — должно быть valid.
    let parsed = doc::json_parse::parse(&resp).expect("response must be valid JSON");
    assert_eq!(parsed.get("jsonrpc").and_then(|v| v.as_str()), Some("2.0"));
    assert_eq!(parsed.get("id").and_then(|v| v.as_int()), Some(1));
    let result = parsed.get("result").expect("result field");
    assert!(result.get("protocolVersion").is_some());
    assert!(result.get("serverInfo").is_some());
}

#[test]
fn mcp_tools_list_well_formed() {
    let tree = tree_json_for("module m\nexport fn f() -> int => 1\n");
    let resp = handle_request(&tree, r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#);
    let parsed = doc::json_parse::parse(&resp).expect("response valid JSON");
    let tools = parsed.get("result").and_then(|r| r.get("tools")).and_then(|v| v.as_array());
    let tools = tools.expect("tools array");
    assert_eq!(tools.len(), 3, "should have 3 tools");
    let names: Vec<&str> = tools.iter()
        .filter_map(|t| t.get("name").and_then(|v| v.as_str()))
        .collect();
    assert!(names.contains(&"query_items"));
    assert!(names.contains(&"list_modules"));
    assert!(names.contains(&"get_item"));
}

#[test]
fn mcp_query_items_returns_results_in_text() {
    let tree = tree_json_for("
module x

export fn alpha() -> int => 1
export fn beta() -> int => 2
");
    let resp = handle_request(&tree,
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"query_items","arguments":{"query":"kind=fn"}}}"#);
    let parsed = doc::json_parse::parse(&resp).expect("valid JSON");
    let result = parsed.get("result").expect("result");
    let content = result.get("content").and_then(|v| v.as_array()).expect("content array");
    assert_eq!(content.len(), 1);
    let text = content[0].get("text").and_then(|v| v.as_str()).expect("text");
    // Embedded text должен содержать query results.
    assert!(text.contains("alpha"));
    assert!(text.contains("beta"));
}

#[test]
fn mcp_list_modules_returns_paths() {
    let tree = tree_json_for("module mymod\nexport fn f() -> int => 1\n");
    let resp = handle_request(&tree,
        r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"list_modules"}}"#);
    let parsed = doc::json_parse::parse(&resp).expect("valid JSON");
    let text = parsed.get("result")
        .and_then(|r| r.get("content"))
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.first())
        .and_then(|c| c.get("text"))
        .and_then(|v| v.as_str())
        .expect("text content");
    assert!(text.contains("mymod"));
}

#[test]
fn mcp_get_item_returns_full_json() {
    let tree = tree_json_for("module m\nexport fn target() -> int => 42\n");
    let resp = handle_request(&tree,
        r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"get_item","arguments":{"item_id":"m::target"}}}"#);
    let parsed = doc::json_parse::parse(&resp).expect("valid JSON");
    let text = parsed.get("result")
        .and_then(|r| r.get("content"))
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.first())
        .and_then(|c| c.get("text"))
        .and_then(|v| v.as_str())
        .expect("text");
    assert!(text.contains("target"));
    assert!(text.contains("m::target"));
}

#[test]
fn mcp_unknown_method_returns_error_with_correct_code() {
    let tree = tree_json_for("module m\nexport fn f() -> int => 1\n");
    let resp = handle_request(&tree, r#"{"jsonrpc":"2.0","id":6,"method":"unknown"}"#);
    let parsed = doc::json_parse::parse(&resp).expect("valid JSON");
    let err = parsed.get("error").expect("error field");
    assert_eq!(err.get("code").and_then(|v| v.as_int()), Some(-32601));
}

#[test]
fn mcp_loop_processes_3_requests() {
    let tree = tree_json_for("module m\nexport fn one() -> int => 1\nexport fn two() -> int => 2\n");
    let input = b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\"}\n\
                  {\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\"}\n\
                  {\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"tools/call\",\"params\":{\"name\":\"list_modules\"}}\n";
    let mut output = Vec::new();
    run_mcp_loop(&tree, &input[..], &mut output).expect("loop run");
    let s = String::from_utf8(output).unwrap();
    let lines: Vec<&str> = s.lines().filter(|l| !l.is_empty()).collect();
    assert_eq!(lines.len(), 3, "expected 3 responses, got {} lines", lines.len());
    for line in &lines {
        doc::json_parse::parse(line).unwrap_or_else(|e| panic!("response not valid JSON: {} | line: {}", e, line));
    }
}
