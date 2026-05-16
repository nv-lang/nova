//! Plan 45 Ф.33.3 — nova.toml `[doc]` section configuration.
//!
//! Minimal TOML subset parser (no serde, no toml crate dep). Reads `[doc]`
//! section из `nova.toml`, returns `DocConfig` struct. CLI args override
//! config values.
//!
//! **Subset supported:**
//! - `[section]` headers
//! - `key = "string"` (с `\\`, `\"`, `\n`, `\t`, `\r` escapes)
//! - `key = integer` (i64)
//! - `key = true | false`
//! - Line comments `# ...`
//! - Blank lines, leading/trailing whitespace ignored
//!
//! **Not supported (intentional MVP):**
//! - Arrays, tables, inline tables
//! - Multiline strings, datetime, floats
//! - Hex/octal/binary integer literals
//! - Unicode escapes
//!
//! **DocConfig fields** (subset overlapping CLI flags):
//! - `strict: bool` — same as `--strict`
//! - `coverage_threshold: Option<u32>` — same as `--coverage-threshold`
//! - `source_url_template: Option<String>` — same as NOVA_DOC_SOURCE_URL_TEMPLATE env
//! - `extern_links: Option<String>` — same as NOVA_DOC_EXTERN_LINKS env
//! - `site_url: Option<String>` — same as NOVA_DOC_SITE_URL env

use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct DocConfig {
    pub strict: bool,
    pub coverage_threshold: Option<u32>,
    pub source_url_template: Option<String>,
    pub extern_links: Option<String>,
    pub site_url: Option<String>,
}

impl DocConfig {
    /// Plan 45 Ф.33.3 — load `[doc]` config из `nova.toml` content.
    ///
    /// Если `[doc]` section отсутствует — returns default. Unknown keys
    /// silently ignored (forward-compat). Malformed TOML → Err with message.
    pub fn from_toml_str(content: &str) -> Result<Self, String> {
        let sections = parse_toml_subset(content)?;
        let doc_section = match sections.get("doc") {
            Some(s) => s,
            None => return Ok(Self::default()),
        };
        let mut cfg = Self::default();
        for (key, val) in doc_section {
            match (key.as_str(), val) {
                ("strict", TomlValue::Bool(b)) => cfg.strict = *b,
                ("coverage_threshold", TomlValue::Int(n)) => {
                    if *n < 0 || *n > 100 {
                        return Err(format!("[doc].coverage_threshold must be 0..=100, got {}", n));
                    }
                    cfg.coverage_threshold = Some(*n as u32);
                }
                ("source_url_template", TomlValue::Str(s)) => {
                    cfg.source_url_template = Some(s.clone());
                }
                ("extern_links", TomlValue::Str(s)) => {
                    cfg.extern_links = Some(s.clone());
                }
                ("site_url", TomlValue::Str(s)) => {
                    cfg.site_url = Some(s.clone());
                }
                // Unknown key in [doc] — skip (forward-compat).
                _ => {}
            }
        }
        Ok(cfg)
    }

    /// Plan 45 Ф.33.3 — apply config к process env vars (для downstream
    /// readers Ф.25.3/Ф.30.1/Ф.31.6 которые читают env).
    ///
    /// Не overrides уже set env vars (CLI args / shell env take priority).
    pub fn apply_env(&self) {
        if let Some(v) = &self.source_url_template {
            if std::env::var("NOVA_DOC_SOURCE_URL_TEMPLATE").is_err() {
                // SAFETY: env mutation — single-threaded process startup.
                unsafe { std::env::set_var("NOVA_DOC_SOURCE_URL_TEMPLATE", v); }
            }
        }
        if let Some(v) = &self.extern_links {
            if std::env::var("NOVA_DOC_EXTERN_LINKS").is_err() {
                unsafe { std::env::set_var("NOVA_DOC_EXTERN_LINKS", v); }
            }
        }
        if let Some(v) = &self.site_url {
            if std::env::var("NOVA_DOC_SITE_URL").is_err() {
                unsafe { std::env::set_var("NOVA_DOC_SITE_URL", v); }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum TomlValue {
    Str(String),
    Int(i64),
    Bool(bool),
}

type SectionMap = BTreeMap<String, BTreeMap<String, TomlValue>>;

/// Plan 45 Ф.33.3 — parse TOML subset (sections + scalar values).
fn parse_toml_subset(content: &str) -> Result<SectionMap, String> {
    let mut sections: SectionMap = BTreeMap::new();
    let mut current_section = String::new(); // "" = top-level (we ignore those)
    sections.insert(String::new(), BTreeMap::new());

    for (line_num, raw_line) in content.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // Section header [name].
        if let Some(name) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            let name = name.trim().to_string();
            if name.is_empty() {
                return Err(format!("line {}: empty section name", line_num + 1));
            }
            current_section = name.clone();
            sections.entry(name).or_default();
            continue;
        }
        // key = value
        let (key, value_str) = line.split_once('=').ok_or_else(|| {
            format!("line {}: expected `key = value`, got `{}`", line_num + 1, line)
        })?;
        let key = key.trim().to_string();
        if key.is_empty() {
            return Err(format!("line {}: empty key", line_num + 1));
        }
        let value_str = strip_inline_comment(value_str.trim());
        let value = parse_value(value_str, line_num + 1)?;
        sections.entry(current_section.clone()).or_default()
            .insert(key, value);
    }
    Ok(sections)
}

fn strip_inline_comment(s: &str) -> &str {
    // Comments after string не считаются — pristine MVP только vне strings.
    // Simple heuristic: find first `#` НЕ внутри string literal.
    let mut in_str = false;
    let mut prev_backslash = false;
    for (i, ch) in s.char_indices() {
        if in_str {
            if ch == '\\' && !prev_backslash {
                prev_backslash = true;
                continue;
            }
            if ch == '"' && !prev_backslash {
                in_str = false;
            }
            prev_backslash = false;
            continue;
        }
        if ch == '"' { in_str = true; continue; }
        if ch == '#' {
            return s[..i].trim_end();
        }
    }
    s
}

fn parse_value(s: &str, line_num: usize) -> Result<TomlValue, String> {
    if s.is_empty() {
        return Err(format!("line {}: empty value", line_num));
    }
    // Boolean.
    if s == "true" { return Ok(TomlValue::Bool(true)); }
    if s == "false" { return Ok(TomlValue::Bool(false)); }
    // String "...".
    if let Some(rest) = s.strip_prefix('"') {
        let inner = rest.strip_suffix('"').ok_or_else(|| {
            format!("line {}: unterminated string", line_num)
        })?;
        return Ok(TomlValue::Str(unescape_string(inner)));
    }
    // Integer.
    if let Ok(n) = s.parse::<i64>() {
        return Ok(TomlValue::Int(n));
    }
    Err(format!("line {}: unsupported value `{}` (expected string, integer, or bool)", line_num, s))
}

fn unescape_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some(other) => { out.push('\\'); out.push(other); }
                None => out.push('\\'),
            }
        } else {
            out.push(ch);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_config_returns_default() {
        let c = DocConfig::from_toml_str("").unwrap();
        assert_eq!(c, DocConfig::default());
    }

    #[test]
    fn no_doc_section_returns_default() {
        let c = DocConfig::from_toml_str("[other]\nx = 1\n").unwrap();
        assert_eq!(c, DocConfig::default());
    }

    #[test]
    fn parses_strict_bool() {
        let c = DocConfig::from_toml_str("[doc]\nstrict = true\n").unwrap();
        assert!(c.strict);
    }

    #[test]
    fn parses_coverage_threshold() {
        let c = DocConfig::from_toml_str("[doc]\ncoverage_threshold = 85\n").unwrap();
        assert_eq!(c.coverage_threshold, Some(85));
    }

    #[test]
    fn coverage_threshold_out_of_range_errors() {
        let r = DocConfig::from_toml_str("[doc]\ncoverage_threshold = 150\n");
        assert!(r.is_err());
        assert!(r.unwrap_err().contains("0..=100"));
    }

    #[test]
    fn parses_source_url_template() {
        let toml = "[doc]\nsource_url_template = \"https://github.com/u/r/blob/main/{path}#L{line}\"\n";
        let c = DocConfig::from_toml_str(toml).unwrap();
        assert!(c.source_url_template.as_deref().unwrap().contains("github.com"));
    }

    #[test]
    fn parses_multiple_fields() {
        let toml = "[doc]\n\
            strict = true\n\
            coverage_threshold = 80\n\
            site_url = \"https://docs.nova-lang.org\"\n";
        let c = DocConfig::from_toml_str(toml).unwrap();
        assert!(c.strict);
        assert_eq!(c.coverage_threshold, Some(80));
        assert_eq!(c.site_url.as_deref(), Some("https://docs.nova-lang.org"));
    }

    #[test]
    fn ignores_unknown_keys() {
        let toml = "[doc]\nstrict = true\nunknown_future_key = \"foo\"\n";
        let c = DocConfig::from_toml_str(toml).unwrap();
        assert!(c.strict);
    }

    #[test]
    fn ignores_comments_and_blank_lines() {
        let toml = "# top comment\n\n[doc]\n# section comment\nstrict = true # inline comment\n\n";
        let c = DocConfig::from_toml_str(toml).unwrap();
        assert!(c.strict);
    }

    #[test]
    fn parses_string_escapes() {
        let toml = "[doc]\nsource_url_template = \"line1\\nline2\"\n";
        let c = DocConfig::from_toml_str(toml).unwrap();
        assert!(c.source_url_template.as_deref().unwrap().contains('\n'));
    }

    #[test]
    fn malformed_missing_equals_errors() {
        let r = DocConfig::from_toml_str("[doc]\nbadline\n");
        assert!(r.is_err());
        assert!(r.unwrap_err().contains("expected `key = value`"));
    }

    #[test]
    fn malformed_unterminated_string_errors() {
        let r = DocConfig::from_toml_str("[doc]\nstrict = \"unterminated\n");
        assert!(r.is_err());
    }
}
