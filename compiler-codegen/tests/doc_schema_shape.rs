//! Plan 45 Ф.21.5 — structural test JSON output ↔ embedded schema.
//!
//! Pure-Rust без зависимостей от json-schema validator'а (heavy crate).
//! Проверяем структуру output'а:
//! - Top-level keys: format_version, nova_version, doc_tests, links,
//!   modules, items.
//! - Каждый item имеет required keys per kind discriminator
//!   (fn → signature, type → definition, const → type/value,
//!   effect/protocol → methods).
//! - schema_v1() — валидный JSON (parseable, has `$defs`).
//!
//! Если эмбэддед schema или output дрейфует — этот тест падает.

use std::path::PathBuf;

/// Минимальный JSON-парсер: возвращает дерево как nested `JsonValue`.
/// Достаточно для navigation по object keys и array elements.
#[derive(Debug)]
enum JsonValue {
    Null,
    Bool(bool),
    Number(String), // raw — без парсинга в float (избегаем потери точности)
    String(String),
    Array(Vec<JsonValue>),
    Object(Vec<(String, JsonValue)>),
}

impl JsonValue {
    fn as_object(&self) -> Option<&[(String, JsonValue)]> {
        match self {
            JsonValue::Object(m) => Some(m),
            _ => None,
        }
    }
    fn as_array(&self) -> Option<&[JsonValue]> {
        match self {
            JsonValue::Array(a) => Some(a),
            _ => None,
        }
    }
    fn as_str(&self) -> Option<&str> {
        match self {
            JsonValue::String(s) => Some(s),
            _ => None,
        }
    }
    fn get(&self, key: &str) -> Option<&JsonValue> {
        self.as_object()?
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v)
    }
}

fn parse_json(s: &str) -> JsonValue {
    let mut p = JsonParser { input: s.as_bytes(), pos: 0 };
    p.skip_ws();
    let v = p.parse_value();
    p.skip_ws();
    v
}

struct JsonParser<'a> {
    input: &'a [u8],
    pos: usize,
}

impl<'a> JsonParser<'a> {
    fn skip_ws(&mut self) {
        while self.pos < self.input.len() {
            let c = self.input[self.pos];
            if c == b' ' || c == b'\t' || c == b'\n' || c == b'\r' {
                self.pos += 1;
            } else {
                break;
            }
        }
    }
    fn parse_value(&mut self) -> JsonValue {
        self.skip_ws();
        let c = self.input[self.pos];
        match c {
            b'{' => self.parse_object(),
            b'[' => self.parse_array(),
            b'"' => JsonValue::String(self.parse_string()),
            b't' | b'f' => self.parse_bool(),
            b'n' => { self.pos += 4; JsonValue::Null }
            _ => self.parse_number(),
        }
    }
    fn parse_object(&mut self) -> JsonValue {
        self.pos += 1; // {
        let mut out = Vec::new();
        loop {
            self.skip_ws();
            if self.input[self.pos] == b'}' {
                self.pos += 1;
                break;
            }
            let k = self.parse_string();
            self.skip_ws();
            assert_eq!(self.input[self.pos], b':');
            self.pos += 1;
            let v = self.parse_value();
            out.push((k, v));
            self.skip_ws();
            if self.input[self.pos] == b',' {
                self.pos += 1;
            }
        }
        JsonValue::Object(out)
    }
    fn parse_array(&mut self) -> JsonValue {
        self.pos += 1; // [
        let mut out = Vec::new();
        loop {
            self.skip_ws();
            if self.input[self.pos] == b']' {
                self.pos += 1;
                break;
            }
            out.push(self.parse_value());
            self.skip_ws();
            if self.input[self.pos] == b',' {
                self.pos += 1;
            }
        }
        JsonValue::Array(out)
    }
    fn parse_string(&mut self) -> String {
        assert_eq!(self.input[self.pos], b'"');
        self.pos += 1;
        let mut out = String::new();
        while self.input[self.pos] != b'"' {
            let c = self.input[self.pos];
            if c == b'\\' {
                self.pos += 1;
                match self.input[self.pos] {
                    b'"' => out.push('"'),
                    b'\\' => out.push('\\'),
                    b'n' => out.push('\n'),
                    b'r' => out.push('\r'),
                    b't' => out.push('\t'),
                    b'/' => out.push('/'),
                    b'u' => {
                        self.pos += 1;
                        let hex = std::str::from_utf8(&self.input[self.pos..self.pos + 4]).unwrap();
                        let code = u32::from_str_radix(hex, 16).unwrap();
                        out.push(char::from_u32(code).unwrap_or('?'));
                        self.pos += 3;
                    }
                    other => panic!("unknown escape: \\{}", other as char),
                }
                self.pos += 1;
            } else {
                // utf-8 sequence — handle as bytes
                let start = self.pos;
                self.pos += 1;
                while self.input[self.pos] & 0xC0 == 0x80 {
                    self.pos += 1;
                }
                out.push_str(std::str::from_utf8(&self.input[start..self.pos]).unwrap());
            }
        }
        self.pos += 1; // closing "
        out
    }
    fn parse_bool(&mut self) -> JsonValue {
        if self.input[self.pos] == b't' {
            self.pos += 4;
            JsonValue::Bool(true)
        } else {
            self.pos += 5;
            JsonValue::Bool(false)
        }
    }
    fn parse_number(&mut self) -> JsonValue {
        let start = self.pos;
        if self.input[self.pos] == b'-' {
            self.pos += 1;
        }
        while self.pos < self.input.len() {
            let c = self.input[self.pos];
            if c.is_ascii_digit() || c == b'.' || c == b'e' || c == b'E' || c == b'+' || c == b'-' {
                self.pos += 1;
            } else {
                break;
            }
        }
        JsonValue::Number(std::str::from_utf8(&self.input[start..self.pos]).unwrap().to_string())
    }
}

fn fixtures_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("nova_tests/doc/fixtures")
}

fn validate_doc_tree(json: &JsonValue, fixture: &str) {
    // Top-level shape
    let obj = json.as_object().unwrap_or_else(|| panic!("{}: not object", fixture));
    let required = ["format_version", "nova_version", "doc_tests", "links", "modules", "items"];
    for k in required {
        assert!(
            obj.iter().any(|(name, _)| name == k),
            "{}: missing top-level key `{}`", fixture, k
        );
    }
    // format_version must be 1
    assert_eq!(
        json.get("format_version").and_then(|v| match v {
            JsonValue::Number(n) => Some(n.clone()),
            _ => None,
        }).as_deref(),
        Some("1"),
        "{}: format_version != 1", fixture
    );
    // Each item must have id, kind, module_path, name, source
    let items = json.get("items").and_then(|v| v.as_array()).unwrap_or(&[]);
    for (idx, it) in items.iter().enumerate() {
        for required_key in ["id", "kind", "module_path", "name", "source"] {
            assert!(
                it.get(required_key).is_some(),
                "{}: item[{}] missing `{}`", fixture, idx, required_key
            );
        }
        // Kind-specific shape
        let kind = it.get("kind").and_then(|v| v.as_str()).unwrap_or("");
        let required_for_kind: &[&str] = match kind {
            "fn" => &["signature"],
            "type" => &["definition"],
            "const" => &["type", "value"],
            "effect" | "protocol" => &["methods"],
            _ => panic!("{}: unknown item kind `{}`", fixture, kind),
        };
        for k in required_for_kind {
            assert!(
                it.get(k).is_some(),
                "{}: item[{}] kind={} missing `{}`", fixture, idx, kind, k
            );
        }
        // Plan 45 Ф.3 / D105: новые поля
        for k in ["aliases", "deprecation", "stability"] {
            assert!(
                it.get(k).is_some(),
                "{}: item[{}] missing `{}` (Plan 45 Ф.3 fields)", fixture, idx, k
            );
        }
    }
    // Each link must have text + target_id (nullable)
    let links = json.get("links").and_then(|v| v.as_array()).unwrap_or(&[]);
    for (idx, l) in links.iter().enumerate() {
        for k in ["text", "target_id", "from_id"] {
            assert!(
                l.get(k).is_some(),
                "{}: link[{}] missing `{}`", fixture, idx, k
            );
        }
    }
    // Each doc-test must have id, index, modifiers
    let tests = json.get("doc_tests").and_then(|v| v.as_array()).unwrap_or(&[]);
    for (idx, t) in tests.iter().enumerate() {
        for k in ["id", "index", "modifiers", "visible_source", "full_source"] {
            assert!(
                t.get(k).is_some(),
                "{}: doc_test[{}] missing `{}`", fixture, idx, k
            );
        }
    }
}

fn check_fixture(name: &str) {
    let path = fixtures_root().join(name).join("expected.json");
    let content = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read {}: {}", path.display(), e));
    let v = parse_json(&content);
    validate_doc_tree(&v, name);
}

#[test]
fn schema_v1_parses() {
    let schema_src = nova_codegen::doc::schema::schema_v1();
    let s = parse_json(schema_src);
    // Schema должна быть object с $defs.
    assert!(s.as_object().is_some(), "schema_v1 not an object");
    assert!(s.get("$defs").is_some(), "schema_v1 missing $defs");
    assert!(s.get("$schema").and_then(|v| v.as_str()).map(|s| s.contains("2020-12")).unwrap_or(false));
    // Critical $defs presence
    let defs = s.get("$defs").unwrap();
    for def_name in ["DocModule", "DocItem", "Signature", "DocLink", "DocTest", "Stability", "Deprecation", "SourceLoc"] {
        assert!(
            defs.get(def_name).is_some(),
            "schema $defs missing `{}`", def_name
        );
    }
}

#[test]
fn fixture_basic_shape() { check_fixture("basic"); }

#[test]
fn fixture_sections_shape() { check_fixture("sections"); }

#[test]
fn fixture_kinds_shape() { check_fixture("kinds"); }

#[test]
fn fixture_links_shape() { check_fixture("links"); }

#[test]
fn fixture_orphan_shape() { check_fixture("orphan"); }

#[test]
fn fixture_doctests_shape() { check_fixture("doctests"); }

#[test]
fn fixture_stability_shape() { check_fixture("stability"); }

#[test]
fn fixture_real_attrs_shape() { check_fixture("real_attrs"); }

#[test]
fn fixture_module_attrs_shape() { check_fixture("module_attrs"); }

#[test]
fn fixture_should_panic_shape() { check_fixture("should_panic"); }
