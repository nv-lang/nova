//! Plan 45 Ф.24.10 — doc semver diff integration tests.
//!
//! Tests the diff logic via `nova doc --diff` by writing temp JSON fixtures
//! and invoking the diff function through a helper that replays the same logic.
//! We test the underlying classification rules via constructed JSON payloads.

/// Build a minimal doc JSON string with specified items.
fn make_doc_json(items: &[(&str, &str, &str, &str)]) -> String {
    // items: (id, visibility, kind, signature_return_type)
    let items_json: Vec<String> = items
        .iter()
        .map(|(id, vis, kind, ret_ty)| {
            format!(
                r#"{{
  "id": {:?},
  "visibility": {:?},
  "kind": {:?},
  "summary": "A summary.",
  "description": null,
  "signature": {{
    "params": [],
    "return_type": {:?},
    "effects": []
  }}
}}"#,
                id, vis, kind, ret_ty
            )
        })
        .collect();
    format!(
        r#"{{
  "format_version": 1,
  "nova_version": "0.1.0",
  "doc_tests": [],
  "links": [],
  "modules": [],
  "items": [{}]
}}"#,
        items_json.join(",\n")
    )
}

fn write_temp(content: &str, name: &str) -> std::path::PathBuf {
    // Use a per-process-unique subdir to avoid races between parallel test threads.
    let pid = std::process::id();
    let tid = format!("{:?}", std::thread::current().id())
        .replace(['(', ')', ' '], "_");
    let dir = std::env::temp_dir()
        .join("nova_semver_diff_tests")
        .join(format!("{}_{}", pid, tid));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, content).unwrap();
    path
}

/// Run nova doc --diff and capture its stdout + exit code.
fn run_diff(old_json: &str, new_json: &str) -> (String, i32) {
    let old_path = write_temp(old_json, "old.json");
    let new_path = write_temp(new_json, "new.json");

    let nova_bin = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("nova-cli/target/release/nova.exe");

    // Fallback to non-.exe on non-Windows.
    let nova_bin = if nova_bin.exists() {
        nova_bin
    } else {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("nova-cli/target/release/nova")
    };

    let output = std::process::Command::new(&nova_bin)
        .args(["doc", "--diff"])
        .arg(&old_path)
        .arg(&new_path)
        .output()
        .unwrap_or_else(|e| panic!("failed to run nova: {}", e));

    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let exit_code = output.status.code().unwrap_or(-1);
    (stdout, exit_code)
}

#[test]
fn no_changes_exit_0() {
    let doc = make_doc_json(&[("mod::foo", "export", "fn", "int")]);
    let (out, code) = run_diff(&doc, &doc);
    assert_eq!(code, 0, "no changes should exit 0; output: {}", out);
    assert!(out.contains("no changes"), "expected 'no changes': {}", out);
}

#[test]
fn removed_item_major_exit_1() {
    let old = make_doc_json(&[("mod::foo", "export", "fn", "int"), ("mod::bar", "export", "fn", "str")]);
    let new = make_doc_json(&[("mod::foo", "export", "fn", "int")]);
    let (out, code) = run_diff(&old, &new);
    assert_eq!(code, 1, "removed item should exit 1 (major): {}", out);
    assert!(out.contains("[major]") && out.contains("mod::bar"), "output: {}", out);
}

#[test]
fn added_item_minor_exit_2() {
    let old = make_doc_json(&[("mod::foo", "export", "fn", "int")]);
    let new = make_doc_json(&[("mod::foo", "export", "fn", "int"), ("mod::bar", "export", "fn", "str")]);
    let (out, code) = run_diff(&old, &new);
    assert_eq!(code, 2, "added item should exit 2 (minor): {}", out);
    assert!(out.contains("[minor]") && out.contains("mod::bar"), "output: {}", out);
}

#[test]
fn signature_change_major_exit_1() {
    let old = make_doc_json(&[("mod::foo", "export", "fn", "int")]);
    let new = make_doc_json(&[("mod::foo", "export", "fn", "str")]);
    let (out, code) = run_diff(&old, &new);
    assert_eq!(code, 1, "signature change should exit 1 (major): {}", out);
    assert!(out.contains("[major]") && out.contains("signature"), "output: {}", out);
}

#[test]
fn docs_only_patch_exit_3() {
    // Same signature, different summary.
    let item_id = "mod::foo";
    let old = format!(
        r#"{{"format_version":1,"nova_version":"0.1.0","doc_tests":[],"links":[],"modules":[],"items":[
        {{"id":{:?},"visibility":"export","kind":"fn","summary":"Old summary","description":null,"signature":{{"params":[],"return_type":"int","effects":[]}}}}
    ]}}"#,
        item_id
    );
    let new = format!(
        r#"{{"format_version":1,"nova_version":"0.1.0","doc_tests":[],"links":[],"modules":[],"items":[
        {{"id":{:?},"visibility":"export","kind":"fn","summary":"New summary","description":null,"signature":{{"params":[],"return_type":"int","effects":[]}}}}
    ]}}"#,
        item_id
    );
    let (out, code) = run_diff(&old, &new);
    assert_eq!(code, 3, "docs-only change should exit 3 (patch): {}", out);
    assert!(out.contains("[patch]"), "expected [patch]: {}", out);
}

#[test]
fn private_item_ignored() {
    // Removing a private item is not a breaking change.
    let old = make_doc_json(&[("mod::foo", "export", "fn", "int"), ("mod::_priv", "private", "fn", "int")]);
    let new = make_doc_json(&[("mod::foo", "export", "fn", "int")]);
    let (out, code) = run_diff(&old, &new);
    assert_eq!(code, 0, "removing private item should exit 0: {}", out);
}
