//! Plan 45 Ф.32.1 — query DSL для filtering DocTree items.
//!
//! Foundation для future MCP server (Ф.32.2): query DSL + filter logic
//! reusable как standalone CLI subcommand или programmatic API.
//!
//! **Query syntax:** comma-separated `key=value` pairs:
//! - `kind=fn` — filter по item kind (fn, type, const, effect, protocol, reexport)
//! - `name=substring` — case-insensitive substring match на item name
//! - `module=path` — exact module path (e.g. `std.io`)
//! - `module-prefix=std` — module starts with this
//! - `capability=pure` — has #pure capability (other: realtime, realtime_nogc)
//! - `capability=forbid:Io` — has forbid for specific effect
//! - `effect=Fs` — fn signature contains this effect
//! - `has-contracts=true` — fn имеет requires/ensures/decreases
//! - `verified=proven` — verify_status (proven, has-counterexample, timeout, not-attempted)
//! - `stability=stable` — stability tier (stable, unstable, experimental)
//! - `deprecated=true` — deprecation present
//!
//! Combine: `kind=fn,capability=pure,name=add` — все 3 conditions (AND).
//!
//! **Output:** Vec<QueryResult> — compact summary per match.

use super::doctree::*;

#[derive(Debug, Clone, PartialEq)]
pub struct QueryResult {
    pub item_id: String,
    pub name: String,
    pub kind: String,
    pub module_path: String,
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Query {
    pub kind: Option<String>,
    pub name_substring: Option<String>,
    pub module_exact: Option<String>,
    pub module_prefix: Option<String>,
    pub capability: Option<String>,
    pub effect_substring: Option<String>,
    pub has_contracts: Option<bool>,
    pub verified: Option<String>,
    pub stability: Option<String>,
    pub deprecated: Option<bool>,
}

/// Plan 45 Ф.32.1 — parse query DSL string в Query struct.
///
/// Format: `key=value,key=value,...`. Unknown keys → error.
pub fn parse_query(s: &str) -> Result<Query, String> {
    let mut q = Query::default();
    if s.trim().is_empty() {
        return Ok(q);
    }
    for pair in s.split(',') {
        let pair = pair.trim();
        if pair.is_empty() { continue; }
        let (key, value) = pair.split_once('=')
            .ok_or_else(|| format!("invalid query fragment `{}` (expected key=value)", pair))?;
        let key = key.trim();
        let value = value.trim();
        match key {
            "kind" => q.kind = Some(value.to_string()),
            "name" => q.name_substring = Some(value.to_lowercase()),
            "module" => q.module_exact = Some(value.to_string()),
            "module-prefix" => q.module_prefix = Some(value.to_string()),
            "capability" => q.capability = Some(value.to_string()),
            "effect" => q.effect_substring = Some(value.to_string()),
            "has-contracts" => q.has_contracts = Some(value == "true"),
            "verified" => q.verified = Some(value.to_string()),
            "stability" => q.stability = Some(value.to_string()),
            "deprecated" => q.deprecated = Some(value == "true"),
            _ => return Err(format!(
                "unknown query key `{}` (allowed: kind, name, module, module-prefix, capability, effect, has-contracts, verified, stability, deprecated)",
                key
            )),
        }
    }
    Ok(q)
}

/// Plan 45 Ф.32.1 — execute query на DocTree, return matched items.
pub fn execute(tree: &DocTree, q: &Query) -> Vec<QueryResult> {
    let mut out = Vec::new();
    for m in &tree.modules {
        let module_path = m.path.join(".");
        if let Some(p) = &q.module_exact {
            if module_path != *p { continue; }
        }
        if let Some(p) = &q.module_prefix {
            if !module_path.starts_with(p) { continue; }
        }
        for it in &m.items {
            if !item_matches(it, q) { continue; }
            out.push(QueryResult {
                item_id: it.id.clone(),
                name: it.name.clone(),
                kind: item_kind_str(&it.kind),
                module_path: module_path.clone(),
                summary: it.summary.clone(),
            });
        }
    }
    out.sort_by(|a, b| a.item_id.cmp(&b.item_id));
    out
}

fn item_matches(it: &DocItem, q: &Query) -> bool {
    // kind filter.
    if let Some(want_kind) = &q.kind {
        if item_kind_str(&it.kind) != *want_kind { return false; }
    }
    // name substring.
    if let Some(name_q) = &q.name_substring {
        if !it.name.to_lowercase().contains(name_q) { return false; }
    }
    // capability filter.
    if let Some(cap_filter) = &q.capability {
        if !capability_matches(&it.capabilities, cap_filter) { return false; }
    }
    // effect substring (только для fn).
    if let Some(eff) = &q.effect_substring {
        let has_effect = match &it.kind {
            ItemKind::Fn(sig) => sig.effects.iter().any(|e| e.name.contains(eff)),
            _ => false,
        };
        if !has_effect { return false; }
    }
    // has-contracts (только для fn).
    if let Some(want) = q.has_contracts {
        let has = match &it.kind {
            ItemKind::Fn(sig) => !sig.contracts.is_empty(),
            _ => false,
        };
        if has != want { return false; }
    }
    // verified status (только для fn).
    if let Some(want) = &q.verified {
        let status = match &it.kind {
            ItemKind::Fn(sig) => verify_status_str(&sig.verify_status),
            _ => "not-applicable",
        };
        if status != want.as_str() { return false; }
    }
    // stability tier.
    if let Some(want) = &q.stability {
        let tier = it.stability.as_ref().map(|s| s.tier.as_str()).unwrap_or("");
        if tier != want.as_str() { return false; }
    }
    // deprecated.
    if let Some(want) = q.deprecated {
        let is_dep = it.deprecation.is_some();
        if is_dep != want { return false; }
    }
    true
}

fn capability_matches(cap: &Capabilities, filter: &str) -> bool {
    // Simple tags: pure, realtime, realtime_nogc.
    // Compound: forbid:EffectName.
    if let Some(eff) = filter.strip_prefix("forbid:") {
        return cap.forbid.iter().any(|f| f == eff);
    }
    if let Some(eff) = filter.strip_prefix("allow_transit:") {
        return cap.allow_transit.iter().any(|f| f == eff);
    }
    match filter {
        "pure" => cap.pure_fn,
        "realtime" => cap.realtime,
        "realtime_nogc" => cap.realtime_nogc,
        _ => false,
    }
}

fn item_kind_str(k: &ItemKind) -> String {
    match k {
        ItemKind::Fn(_) => "fn".to_string(),
        ItemKind::Type(_) => "type".to_string(),
        ItemKind::Const { .. } => "const".to_string(),
        ItemKind::Effect { .. } => "effect".to_string(),
        ItemKind::Protocol { .. } => "protocol".to_string(),
        ItemKind::ReExport { .. } => "reexport".to_string(),
    }
}

fn verify_status_str(s: &VerifyStatus) -> &'static str {
    match s {
        VerifyStatus::Proven => "proven",
        VerifyStatus::HasCounterexample(_) => "has-counterexample",
        VerifyStatus::Timeout => "timeout",
        VerifyStatus::NotAttempted => "not-attempted",
    }
}

/// Plan 45 Ф.32.1 — render QueryResult'ы как compact JSON array.
pub fn render_results_json(results: &[QueryResult]) -> String {
    let mut out = String::with_capacity(256);
    out.push_str("[\n");
    for (i, r) in results.iter().enumerate() {
        if i > 0 { out.push_str(",\n"); }
        let summary_field = match &r.summary {
            Some(s) => format!("{:?}", s),
            None => "null".to_string(),
        };
        let _ = std::fmt::Write::write_fmt(&mut out, format_args!(
            "  {{\"item_id\": {:?}, \"name\": {:?}, \"kind\": {:?}, \"module_path\": {:?}, \"summary\": {}}}",
            r.item_id, r.name, r.kind, r.module_path, summary_field
        ));
    }
    out.push_str("\n]\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_empty() {
        let q = parse_query("").unwrap();
        assert!(q.kind.is_none());
    }

    #[test]
    fn parse_single_kind() {
        let q = parse_query("kind=fn").unwrap();
        assert_eq!(q.kind.as_deref(), Some("fn"));
    }

    #[test]
    fn parse_multiple() {
        let q = parse_query("kind=fn,capability=pure,name=add").unwrap();
        assert_eq!(q.kind.as_deref(), Some("fn"));
        assert_eq!(q.capability.as_deref(), Some("pure"));
        assert_eq!(q.name_substring.as_deref(), Some("add"));
    }

    #[test]
    fn parse_unknown_key_errors() {
        let r = parse_query("foo=bar");
        assert!(r.is_err());
        assert!(r.unwrap_err().contains("unknown query key"));
    }

    #[test]
    fn parse_missing_equals_errors() {
        let r = parse_query("noequals");
        assert!(r.is_err());
    }
}
