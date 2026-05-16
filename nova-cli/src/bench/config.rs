// SPDX-License-Identifier: MIT OR Apache-2.0
//! Plan 57 L6 — `bench.toml` configuration parser.
//!
//! Минимальный TOML parser inline, без внешнего `toml` crate
//! (политика минимума deps в Cargo.toml). Поддерживает только то, что
//! нужно для `[gate]`, `[gate.exempt]`, `[gate.strict]`:
//!   - sections [section] и [section.subsection]
//!   - key = value (float, int, bool, string, array of strings)
//!   - comments #
//!
//! Не поддерживает: inline tables, dotted keys, multi-line strings, dates.
//! Если файл содержит unsupported syntax — silent skip line (degrades
//! gracefully), parse-errors писать в `parse_errors` для diag.
//!
//! Default config — соответствует spec Plan 57 §3 L6.

use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct BenchToml {
    pub gate: GateConfig,
    pub strict_overrides: HashMap<String, GateConfig>,
    pub exempt_globs: Vec<String>,
    pub parse_errors: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct GateConfig {
    pub wall_clock_delta_pct: f64,
    pub allocs_delta_pct: f64,
    pub gc_pause_delta_pct: f64,
    pub significance_p: f64,
    pub auto_noise_floor: bool,
    pub noise_floor_calibration_runs: u32,
}

impl Default for GateConfig {
    fn default() -> Self {
        Self {
            wall_clock_delta_pct: 5.0,
            allocs_delta_pct: 10.0,
            gc_pause_delta_pct: 20.0,
            significance_p: 0.01,
            auto_noise_floor: true,
            noise_floor_calibration_runs: 5,
        }
    }
}

impl Default for BenchToml {
    fn default() -> Self {
        Self {
            gate: GateConfig::default(),
            strict_overrides: HashMap::new(),
            exempt_globs: Vec::new(),
            parse_errors: Vec::new(),
        }
    }
}

impl BenchToml {
    /// Load from path. Если файл отсутствует — возвращает Default.
    /// Parse errors не fatal — logged в parse_errors.
    pub fn load_or_default(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(s) => Self::parse(&s),
            Err(_) => Self::default(),
        }
    }

    pub fn parse(input: &str) -> Self {
        let mut cfg = Self::default();
        let mut current_section: Vec<String> = Vec::new();
        let mut current_strict_name: Option<String> = None;

        for (lineno, raw_line) in input.lines().enumerate() {
            let line = raw_line.split('#').next().unwrap_or("").trim();
            if line.is_empty() { continue; }

            // Section header: [a] or [a.b].
            if let Some(rest) = line.strip_prefix('[') {
                if let Some(name) = rest.strip_suffix(']') {
                    current_section = name.trim().split('.').map(|s| s.trim().to_string()).collect();
                    // Special: [gate.strict.<name>] — per-bench strict override.
                    if current_section.len() == 3 && current_section[0] == "gate" && current_section[1] == "strict" {
                        current_strict_name = Some(current_section[2].clone());
                        if !cfg.strict_overrides.contains_key(&current_section[2]) {
                            cfg.strict_overrides.insert(current_section[2].clone(), GateConfig::default());
                        }
                    } else {
                        current_strict_name = None;
                    }
                    continue;
                }
            }

            // key = value
            if let Some(eq_pos) = line.find('=') {
                let key = line[..eq_pos].trim();
                let value = line[eq_pos + 1..].trim();
                if let Err(e) = cfg.apply_kv(&current_section, current_strict_name.as_deref(), key, value) {
                    cfg.parse_errors.push(format!("line {}: {}", lineno + 1, e));
                }
                continue;
            }

            cfg.parse_errors.push(format!("line {}: unrecognized syntax: `{}`", lineno + 1, line));
        }
        cfg
    }

    fn apply_kv(&mut self,
                section: &[String],
                strict_name: Option<&str>,
                key: &str,
                value: &str) -> Result<(), String> {
        // Strip outer quotes if string.
        let unquoted = |s: &str| -> Option<String> {
            if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
                Some(s[1..s.len() - 1].to_string())
            } else { None }
        };
        let parse_bool = |s: &str| -> Option<bool> {
            match s { "true" => Some(true), "false" => Some(false), _ => None }
        };
        let parse_float = |s: &str| -> Option<f64> { s.parse().ok() };
        let parse_array_str = |s: &str| -> Option<Vec<String>> {
            let s = s.trim();
            if !s.starts_with('[') || !s.ends_with(']') { return None; }
            let inner = &s[1..s.len() - 1];
            let mut out = Vec::new();
            for part in inner.split(',') {
                let p = part.trim();
                if p.is_empty() { continue; }
                if let Some(u) = unquoted(p) { out.push(u); } else { return None; }
            }
            Some(out)
        };

        // Apply to top-level [gate] or per-bench [gate.strict.<name>].
        let target: &mut GateConfig = if let Some(name) = strict_name {
            self.strict_overrides.get_mut(name).unwrap()
        } else if section.len() == 1 && section[0] == "gate" {
            &mut self.gate
        } else if section.len() == 2 && section[0] == "gate" && section[1] == "exempt" {
            // [gate.exempt] section — only `benches = [...]` key.
            if key == "benches" {
                if let Some(arr) = parse_array_str(value) {
                    self.exempt_globs = arr;
                    return Ok(());
                }
                return Err(format!("expected array of strings for `benches`, got `{}`", value));
            }
            return Err(format!("unknown key `{}` in [gate.exempt]", key));
        } else {
            // Unknown section — skip silently.
            return Ok(());
        };

        match key {
            "wall_clock_delta_pct" => target.wall_clock_delta_pct = parse_float(value)
                .ok_or_else(|| format!("expected float for `{}`, got `{}`", key, value))?,
            "allocs_delta_pct" => target.allocs_delta_pct = parse_float(value)
                .ok_or_else(|| format!("expected float for `{}`, got `{}`", key, value))?,
            "gc_pause_delta_pct" => target.gc_pause_delta_pct = parse_float(value)
                .ok_or_else(|| format!("expected float for `{}`, got `{}`", key, value))?,
            "significance_p" => target.significance_p = parse_float(value)
                .ok_or_else(|| format!("expected float for `{}`, got `{}`", key, value))?,
            "auto_noise_floor" => target.auto_noise_floor = parse_bool(value)
                .ok_or_else(|| format!("expected bool for `{}`, got `{}`", key, value))?,
            "noise_floor_calibration_runs" => {
                let f = parse_float(value)
                    .ok_or_else(|| format!("expected int for `{}`, got `{}`", key, value))?;
                target.noise_floor_calibration_runs = f as u32;
            },
            _ => return Err(format!("unknown key `{}`", key)),
        }
        Ok(())
    }

    /// Resolve effective gate for a bench name (apply strict override if matches).
    pub fn gate_for(&self, bench_name: &str) -> &GateConfig {
        // Check strict overrides via glob match.
        for (pattern, cfg) in &self.strict_overrides {
            if glob_match(pattern, bench_name) {
                return cfg;
            }
        }
        &self.gate
    }

    /// True если bench освобождён от gate (exempt list).
    pub fn is_exempt(&self, bench_name: &str) -> bool {
        self.exempt_globs.iter().any(|p| glob_match(p, bench_name))
    }
}

/// Simple glob matcher: `*` matches any substring, `?` any single char.
pub fn glob_match(pattern: &str, s: &str) -> bool {
    // Special case — exact match.
    if !pattern.contains('*') && !pattern.contains('?') {
        return pattern == s;
    }
    glob_recursive(pattern.as_bytes(), s.as_bytes())
}

fn glob_recursive(pat: &[u8], s: &[u8]) -> bool {
    let mut pi = 0usize;
    let mut si = 0usize;
    let mut star_pi: Option<usize> = None;
    let mut star_si = 0usize;
    while si < s.len() {
        if pi < pat.len() && (pat[pi] == s[si] || pat[pi] == b'?') {
            pi += 1; si += 1;
        } else if pi < pat.len() && pat[pi] == b'*' {
            star_pi = Some(pi);
            star_si = si;
            pi += 1;
        } else if let Some(spi) = star_pi {
            pi = spi + 1;
            star_si += 1;
            si = star_si;
        } else {
            return false;
        }
    }
    while pi < pat.len() && pat[pi] == b'*' { pi += 1; }
    pi == pat.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_applied() {
        let cfg = BenchToml::default();
        assert_eq!(cfg.gate.wall_clock_delta_pct, 5.0);
        assert_eq!(cfg.gate.significance_p, 0.01);
        assert!(cfg.gate.auto_noise_floor);
    }

    #[test]
    fn parse_basic() {
        let input = r#"
[gate]
wall_clock_delta_pct = 3.0
allocs_delta_pct = 15.0
significance_p = 0.05
auto_noise_floor = false
noise_floor_calibration_runs = 10

[gate.exempt]
benches = ["sleep_*", "io_*"]

[gate.strict.parse_1k]
wall_clock_delta_pct = 1.5
"#;
        let cfg = BenchToml::parse(input);
        assert!(cfg.parse_errors.is_empty(), "errors: {:?}", cfg.parse_errors);
        assert_eq!(cfg.gate.wall_clock_delta_pct, 3.0);
        assert_eq!(cfg.gate.significance_p, 0.05);
        assert_eq!(cfg.gate.allocs_delta_pct, 15.0);
        assert!(!cfg.gate.auto_noise_floor);
        assert_eq!(cfg.exempt_globs, vec!["sleep_*", "io_*"]);
        let strict = cfg.gate_for("parse_1k");
        assert_eq!(strict.wall_clock_delta_pct, 1.5);
        // Other benches → default.
        let other = cfg.gate_for("hashmap_insert");
        assert_eq!(other.wall_clock_delta_pct, 3.0);
    }

    #[test]
    fn glob_matches() {
        assert!(glob_match("sleep_*", "sleep_10ms"));
        assert!(glob_match("*_io_*", "test_io_read"));
        assert!(glob_match("exact", "exact"));
        assert!(!glob_match("sleep_*", "no_sleep"));
        assert!(glob_match("a?c", "abc"));
        assert!(!glob_match("a?c", "abcd"));
    }

    #[test]
    fn exempt_check() {
        let mut cfg = BenchToml::default();
        cfg.exempt_globs = vec!["sleep_*".to_string()];
        assert!(cfg.is_exempt("sleep_test"));
        assert!(!cfg.is_exempt("hashmap_test"));
    }

    #[test]
    fn comments_and_blanks() {
        let input = r#"
# Header comment
[gate]   # trailing comment
wall_clock_delta_pct = 7.0  # inline

# Empty line above
"#;
        let cfg = BenchToml::parse(input);
        assert!(cfg.parse_errors.is_empty());
        assert_eq!(cfg.gate.wall_clock_delta_pct, 7.0);
    }
}
