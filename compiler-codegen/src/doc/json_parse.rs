//! Plan 45 Ф.32.2 — minimal JSON parser для doc-query JSON input.
//!
//! **Цель:** parse `nova doc --format json` output чтобы doc-query subcommand
//! мог принимать pre-generated JSON (вместо re-parsing .nv каждый раз).
//!
//! **НЕ полный JSON parser** — focuses на shape которую emit'ит render_json:
//! - Objects (`{...}`), arrays (`[...]`)
//! - Strings (с `\\`, `\"`, `\n`, `\t`, `\r` escapes — то что мы emit'им)
//! - Numbers (integer literals только — мы emit'им u32 в format_version и т.п.)
//! - Booleans (`true`, `false`)
//! - Null (`null`)
//!
//! **Не поддержано:**
//! - Floats (мы не emit'им — все числа integer)
//! - Unicode escapes `\uXXXX` (мы не emit'им — utf8 raw)
//! - Trailing commas (мы не emit'им)
//!
//! **API:** `parse(json) -> Result<JsonValue, String>` + helpers
//! `as_object`, `as_array`, `as_str`, `as_int`, `as_bool` для traversal.

use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub enum JsonValue {
    Null,
    Bool(bool),
    Int(i64),
    Str(String),
    Array(Vec<JsonValue>),
    Object(BTreeMap<String, JsonValue>),
}

impl JsonValue {
    pub fn as_object(&self) -> Option<&BTreeMap<String, JsonValue>> {
        if let JsonValue::Object(m) = self { Some(m) } else { None }
    }
    pub fn as_array(&self) -> Option<&Vec<JsonValue>> {
        if let JsonValue::Array(a) = self { Some(a) } else { None }
    }
    pub fn as_str(&self) -> Option<&str> {
        if let JsonValue::Str(s) = self { Some(s) } else { None }
    }
    pub fn as_int(&self) -> Option<i64> {
        if let JsonValue::Int(n) = self { Some(*n) } else { None }
    }
    pub fn as_bool(&self) -> Option<bool> {
        if let JsonValue::Bool(b) = self { Some(*b) } else { None }
    }
    pub fn is_null(&self) -> bool {
        matches!(self, JsonValue::Null)
    }
    /// Convenience: object.get(key) → Option<&JsonValue>.
    pub fn get(&self, key: &str) -> Option<&JsonValue> {
        self.as_object().and_then(|m| m.get(key))
    }
}

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(s: &'a str) -> Self {
        Self { bytes: s.as_bytes(), pos: 0 }
    }

    fn err(&self, msg: &str) -> String {
        format!("JSON parse error at byte {}: {}", self.pos, msg)
    }

    fn skip_ws(&mut self) {
        while self.pos < self.bytes.len() {
            match self.bytes[self.pos] {
                b' ' | b'\t' | b'\n' | b'\r' => self.pos += 1,
                _ => break,
            }
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn consume(&mut self, expected: u8) -> Result<(), String> {
        if self.peek() != Some(expected) {
            return Err(self.err(&format!("expected `{}`, got {:?}",
                expected as char, self.peek().map(|b| b as char))));
        }
        self.pos += 1;
        Ok(())
    }

    fn parse_value(&mut self) -> Result<JsonValue, String> {
        self.skip_ws();
        match self.peek() {
            None => Err(self.err("unexpected end of input")),
            Some(b'{') => self.parse_object(),
            Some(b'[') => self.parse_array(),
            Some(b'"') => self.parse_string().map(JsonValue::Str),
            Some(b't') | Some(b'f') => self.parse_bool(),
            Some(b'n') => self.parse_null(),
            Some(b'-') | Some(b'0'..=b'9') => self.parse_number(),
            Some(b) => Err(self.err(&format!("unexpected byte {:?}", b as char))),
        }
    }

    fn parse_object(&mut self) -> Result<JsonValue, String> {
        self.consume(b'{')?;
        let mut map = BTreeMap::new();
        self.skip_ws();
        if self.peek() == Some(b'}') {
            self.pos += 1;
            return Ok(JsonValue::Object(map));
        }
        loop {
            self.skip_ws();
            let key = self.parse_string()?;
            self.skip_ws();
            self.consume(b':')?;
            let value = self.parse_value()?;
            map.insert(key, value);
            self.skip_ws();
            match self.peek() {
                Some(b',') => { self.pos += 1; continue; }
                Some(b'}') => { self.pos += 1; return Ok(JsonValue::Object(map)); }
                _ => return Err(self.err("expected `,` or `}` in object")),
            }
        }
    }

    fn parse_array(&mut self) -> Result<JsonValue, String> {
        self.consume(b'[')?;
        let mut arr = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b']') {
            self.pos += 1;
            return Ok(JsonValue::Array(arr));
        }
        loop {
            let value = self.parse_value()?;
            arr.push(value);
            self.skip_ws();
            match self.peek() {
                Some(b',') => { self.pos += 1; continue; }
                Some(b']') => { self.pos += 1; return Ok(JsonValue::Array(arr)); }
                _ => return Err(self.err("expected `,` or `]` in array")),
            }
        }
    }

    fn parse_string(&mut self) -> Result<String, String> {
        self.consume(b'"')?;
        let mut out = String::new();
        while self.pos < self.bytes.len() {
            let b = self.bytes[self.pos];
            if b == b'"' {
                self.pos += 1;
                return Ok(out);
            }
            if b == b'\\' {
                self.pos += 1;
                let next = self.peek().ok_or_else(|| self.err("trailing backslash"))?;
                match next {
                    b'"' => out.push('"'),
                    b'\\' => out.push('\\'),
                    b'/' => out.push('/'),
                    b'n' => out.push('\n'),
                    b't' => out.push('\t'),
                    b'r' => out.push('\r'),
                    b'b' => out.push('\u{0008}'),
                    b'f' => out.push('\u{000C}'),
                    b'u' => {
                        return Err(self.err("\\uXXXX escape unsupported (Plan 45 Ф.32.2 limitation)"));
                    }
                    other => return Err(self.err(&format!("invalid escape \\{}", other as char))),
                }
                self.pos += 1;
                continue;
            }
            // Raw UTF-8 byte — push as-is.
            out.push(b as char);
            self.pos += 1;
        }
        Err(self.err("unterminated string"))
    }

    fn parse_bool(&mut self) -> Result<JsonValue, String> {
        if self.bytes[self.pos..].starts_with(b"true") {
            self.pos += 4;
            Ok(JsonValue::Bool(true))
        } else if self.bytes[self.pos..].starts_with(b"false") {
            self.pos += 5;
            Ok(JsonValue::Bool(false))
        } else {
            Err(self.err("expected `true` or `false`"))
        }
    }

    fn parse_null(&mut self) -> Result<JsonValue, String> {
        if self.bytes[self.pos..].starts_with(b"null") {
            self.pos += 4;
            Ok(JsonValue::Null)
        } else {
            Err(self.err("expected `null`"))
        }
    }

    fn parse_number(&mut self) -> Result<JsonValue, String> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        while let Some(b) = self.peek() {
            if b.is_ascii_digit() {
                self.pos += 1;
            } else {
                break;
            }
        }
        let s = std::str::from_utf8(&self.bytes[start..self.pos])
            .map_err(|_| self.err("invalid UTF-8 in number"))?;
        s.parse::<i64>()
            .map(JsonValue::Int)
            .map_err(|e| self.err(&format!("invalid integer: {}", e)))
    }
}

/// Plan 45 Ф.32.2 — parse JSON string into JsonValue tree.
pub fn parse(input: &str) -> Result<JsonValue, String> {
    let mut p = Parser::new(input);
    let v = p.parse_value()?;
    p.skip_ws();
    if p.pos != p.bytes.len() {
        return Err(p.err("trailing content after JSON value"));
    }
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_null() {
        assert_eq!(parse("null").unwrap(), JsonValue::Null);
    }

    #[test]
    fn parse_booleans() {
        assert_eq!(parse("true").unwrap(), JsonValue::Bool(true));
        assert_eq!(parse("false").unwrap(), JsonValue::Bool(false));
    }

    #[test]
    fn parse_integers() {
        assert_eq!(parse("42").unwrap(), JsonValue::Int(42));
        assert_eq!(parse("-100").unwrap(), JsonValue::Int(-100));
        assert_eq!(parse("0").unwrap(), JsonValue::Int(0));
    }

    #[test]
    fn parse_strings() {
        assert_eq!(parse("\"hello\"").unwrap(), JsonValue::Str("hello".to_string()));
        assert_eq!(parse("\"\"").unwrap(), JsonValue::Str("".to_string()));
        assert_eq!(parse("\"\\\"escaped\\\"\"").unwrap(),
            JsonValue::Str("\"escaped\"".to_string()));
        assert_eq!(parse("\"with\\nnewline\"").unwrap(),
            JsonValue::Str("with\nnewline".to_string()));
    }

    #[test]
    fn parse_empty_array() {
        assert_eq!(parse("[]").unwrap(), JsonValue::Array(vec![]));
    }

    #[test]
    fn parse_array_of_ints() {
        let v = parse("[1, 2, 3]").unwrap();
        let arr = v.as_array().unwrap();
        assert_eq!(arr.len(), 3);
        assert_eq!(arr[0].as_int(), Some(1));
    }

    #[test]
    fn parse_empty_object() {
        assert_eq!(parse("{}").unwrap(), JsonValue::Object(BTreeMap::new()));
    }

    #[test]
    fn parse_simple_object() {
        let v = parse("{\"name\": \"foo\", \"count\": 42}").unwrap();
        assert_eq!(v.get("name").unwrap().as_str(), Some("foo"));
        assert_eq!(v.get("count").unwrap().as_int(), Some(42));
    }

    #[test]
    fn parse_nested() {
        let v = parse("{\"items\": [{\"id\": 1}, {\"id\": 2}]}").unwrap();
        let items = v.get("items").unwrap().as_array().unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].get("id").unwrap().as_int(), Some(1));
    }

    #[test]
    fn parse_with_whitespace() {
        let v = parse("  {\n  \"a\": 1\n}  ").unwrap();
        assert_eq!(v.get("a").unwrap().as_int(), Some(1));
    }

    #[test]
    fn parse_null_value_in_object() {
        let v = parse("{\"x\": null}").unwrap();
        assert!(v.get("x").unwrap().is_null());
    }

    #[test]
    fn parse_trailing_content_errors() {
        let r = parse("42 extra");
        assert!(r.is_err());
        assert!(r.unwrap_err().contains("trailing content"));
    }

    #[test]
    fn parse_unterminated_string_errors() {
        assert!(parse("\"foo").is_err());
    }

    #[test]
    fn parse_invalid_top_level_errors() {
        assert!(parse("blah").is_err());
    }
}
