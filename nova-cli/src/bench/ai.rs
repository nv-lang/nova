// SPDX-License-Identifier: MIT OR Apache-2.0
//! Plan 57.F.2 — AI-driven regression interpretation.
//!
//! Opt-in: `nova bench diff baseline.json new.json --explain` sends
//! structured diff + git log context to an LLM API and prints a
//! natural-language interpretation of likely root cause.
//!
//! No Rust HTTP/TLS dependency: uses system `curl` (cross-platform,
//! Win10+/macOS/Linux ship it). Privacy: opt-in flag только.
//!
//! Config sources (in priority order):
//!   1. NOVA_AI_API_KEY env (required)
//!   2. NOVA_AI_PROVIDER env (anthropic|openai, default: anthropic)
//!   3. NOVA_AI_MODEL env (default per-provider)
//!   4. ~/.nova-ai.toml (overrides if present)
//!   5. --max-tokens CLI flag (default 4000)

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{anyhow, bail, Result};
use serde_json::{json, Value};

use super::diff::DiffRow;

pub const DEFAULT_CONFIG_PATH: &str = ".nova-ai.toml";

#[derive(Debug, Clone)]
pub enum Provider {
    Anthropic,
    OpenAi,
}

impl Provider {
    pub fn parse(s: &str) -> Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "anthropic" | "claude" => Ok(Provider::Anthropic),
            "openai" | "gpt"       => Ok(Provider::OpenAi),
            _ => Err(anyhow!("unknown AI provider: `{}` (expected anthropic|openai)", s)),
        }
    }

    pub fn default_model(&self) -> &'static str {
        match self {
            Provider::Anthropic => "claude-opus-4-7",
            Provider::OpenAi    => "gpt-4o-mini",
        }
    }
}

#[derive(Debug, Clone)]
pub struct AiConfig {
    pub provider: Provider,
    pub model: String,
    pub api_key: String,
    pub max_tokens: u32,
    pub include_git_diff: bool,
    pub include_commits: bool,
    pub max_commits: usize,
}

impl AiConfig {
    /// Load config: env first, then ~/.nova-ai.toml overrides.
    pub fn load(explicit_config: Option<&Path>, max_tokens_override: Option<u32>)
        -> Result<Self>
    {
        let mut provider = std::env::var("NOVA_AI_PROVIDER").ok()
            .map(|s| Provider::parse(&s))
            .transpose()?
            .unwrap_or(Provider::Anthropic);
        let mut model: Option<String> = std::env::var("NOVA_AI_MODEL").ok();
        let api_key = std::env::var("NOVA_AI_API_KEY")
            .map_err(|_| anyhow!("NOVA_AI_API_KEY env var not set — required for --explain"))?;
        let mut max_tokens: u32 = 4000;
        let mut include_git_diff = true;
        let mut include_commits = true;
        let mut max_commits = 20;

        // TOML override pass.
        let path = explicit_config.map(|p| p.to_path_buf())
            .or_else(|| std::env::var_os("HOME")
                .or_else(|| std::env::var_os("USERPROFILE"))
                .map(|h| PathBuf::from(h).join(DEFAULT_CONFIG_PATH)));
        if let Some(p) = path.as_ref() {
            if p.exists() {
                let text = std::fs::read_to_string(p)
                    .map_err(|e| anyhow!("read {}: {}", p.display(), e))?;
                for raw in text.lines() {
                    let line = raw.split('#').next().unwrap_or("").trim();
                    if line.is_empty() || line.starts_with('[') { continue; }
                    if let Some(eq) = line.find('=') {
                        let key = line[..eq].trim();
                        let val = line[eq + 1..].trim();
                        let unquoted = val.trim_matches('"').to_string();
                        match key {
                            "provider"         => provider = Provider::parse(&unquoted)?,
                            "model"            => model = Some(unquoted),
                            "max_tokens"       => max_tokens = unquoted.parse()
                                .map_err(|_| anyhow!("max_tokens parse: {}", unquoted))?,
                            "include_git_diff" => include_git_diff = unquoted == "true",
                            "include_commits"  => include_commits  = unquoted == "true",
                            "max_commits"      => max_commits = unquoted.parse()
                                .map_err(|_| anyhow!("max_commits parse: {}", unquoted))?,
                            _ => {}  // silently ignore unknown keys для forward-compat
                        }
                    }
                }
            }
        }
        if let Some(mt) = max_tokens_override { max_tokens = mt; }
        let model_str = model.unwrap_or_else(|| provider.default_model().to_string());
        Ok(Self {
            provider, model: model_str, api_key, max_tokens,
            include_git_diff, include_commits, max_commits,
        })
    }
}

/// Build a structured context blob (used as user message body).
pub struct PromptBuilder<'a> {
    pub diff_rows: &'a [DiffRow],
    pub git_diff: Option<String>,
    pub git_commits: Option<String>,
    pub note: Option<&'a str>,
}

impl<'a> PromptBuilder<'a> {
    /// Render the prompt body as plain XML-tagged text. Tags help LLM
    /// segment context cleanly; matches Anthropic's recommended pattern.
    pub fn build(&self) -> String {
        let mut s = String::with_capacity(4096);
        s.push_str("You are an expert performance engineer analyzing a Nova-language benchmark regression. \
Read the structured data below and explain the most likely root cause in 5-12 sentences. \
Be concrete: name specific files, functions, or commit SHAs when the evidence supports it. \
End with a `Confidence: low|medium|high` line and 1-3 `Recommended actions:` bullets.\n\n");

        s.push_str("<bench_diff_table>\n");
        s.push_str("name | baseline_ns | new_ns | delta_pct | p_value | n_base | n_new\n");
        for r in self.diff_rows {
            s.push_str(&format!(
                "{} | {} | {} | {} | {} | {} | {}\n",
                r.name,
                fmt_opt(&r.baseline_median_ns),
                fmt_opt(&r.new_median_ns),
                fmt_opt_pct(&r.delta_pct),
                fmt_opt_p(&r.p_value),
                r.n_baseline,
                r.n_new,
            ));
        }
        s.push_str("</bench_diff_table>\n\n");

        if let Some(d) = &self.git_diff {
            s.push_str("<git_diff>\n");
            // Truncate huge diffs to avoid token explosion.
            let truncated = truncate_for_tokens(d, 12_000);
            s.push_str(&truncated);
            if d.len() > truncated.len() {
                s.push_str("\n[... diff truncated for token budget ...]\n");
            }
            s.push_str("\n</git_diff>\n\n");
        }
        if let Some(c) = &self.git_commits {
            s.push_str("<git_commits>\n");
            s.push_str(c);
            s.push_str("\n</git_commits>\n\n");
        }
        if let Some(n) = self.note {
            s.push_str("<additional_note>\n");
            s.push_str(n);
            s.push_str("\n</additional_note>\n\n");
        }
        s
    }
}

fn fmt_opt(v: &Option<f64>) -> String {
    v.map(|x| format!("{:.1}", x)).unwrap_or_else(|| "-".to_string())
}
fn fmt_opt_pct(v: &Option<f64>) -> String {
    v.map(|x| format!("{:+.2}%", x)).unwrap_or_else(|| "-".to_string())
}
fn fmt_opt_p(v: &Option<f64>) -> String {
    v.map(|x| format!("{:.4}", x)).unwrap_or_else(|| "-".to_string())
}

/// Truncate string at ~max_chars boundary (whole lines only).
fn truncate_for_tokens(s: &str, max_chars: usize) -> String {
    if s.len() <= max_chars { return s.to_string(); }
    let mut out = String::with_capacity(max_chars);
    for line in s.lines() {
        if out.len() + line.len() + 1 > max_chars { break; }
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// Collect git context (diff + commit log) between two SHAs.
/// Returns (diff, commits) — both Option (None if `git` fails or SHAs missing).
pub fn collect_git_context(repo: &Path, baseline_sha: Option<&str>,
                           new_sha: Option<&str>, max_commits: usize)
    -> (Option<String>, Option<String>)
{
    let (base, new) = match (baseline_sha, new_sha) {
        (Some(b), Some(n)) if !b.is_empty() && !n.is_empty() => (b, n),
        _ => return (None, None),
    };
    let range = format!("{}..{}", base, new);
    let diff = Command::new("git")
        .args(["-C", &repo.to_string_lossy(), "diff", "--stat", "--patch", &range])
        .output().ok()
        .and_then(|o| if o.status.success() {
            Some(String::from_utf8_lossy(&o.stdout).to_string())
        } else { None });
    let commits = Command::new("git")
        .args(["-C", &repo.to_string_lossy(), "log",
               &format!("-n{}", max_commits),
               "--pretty=format:%h %s%n%b%n---", &range])
        .output().ok()
        .and_then(|o| if o.status.success() {
            Some(String::from_utf8_lossy(&o.stdout).to_string())
        } else { None });
    (diff, commits)
}

/// LLM call result.
#[derive(Debug, Clone)]
pub struct AiResponse {
    pub text: String,
    pub tokens_used: Option<u32>,
    pub model: String,
    pub provider: String,
}

/// Send the prompt to the configured LLM provider via system `curl`.
/// `dry_run` skips the API call and returns the would-be request body
/// (для cost estimation / debugging).
pub fn call_api(cfg: &AiConfig, prompt: &str, dry_run: bool) -> Result<AiResponse> {
    match cfg.provider {
        Provider::Anthropic => call_anthropic(cfg, prompt, dry_run),
        Provider::OpenAi    => call_openai(cfg, prompt, dry_run),
    }
}

fn call_anthropic(cfg: &AiConfig, prompt: &str, dry_run: bool) -> Result<AiResponse> {
    let body = json!({
        "model": cfg.model,
        "max_tokens": cfg.max_tokens,
        "messages": [{ "role": "user", "content": prompt }],
    });
    if dry_run {
        return Ok(AiResponse {
            text: format!("DRY RUN — would POST к https://api.anthropic.com/v1/messages with body:\n{}",
                serde_json::to_string_pretty(&body)?),
            tokens_used: None,
            model: cfg.model.clone(),
            provider: "anthropic".to_string(),
        });
    }
    let body_str = serde_json::to_string(&body)?;
    let out = curl_post("https://api.anthropic.com/v1/messages", &body_str, &[
        ("x-api-key", &cfg.api_key),
        ("anthropic-version", "2023-06-01"),
        ("content-type", "application/json"),
    ])?;
    let v: Value = serde_json::from_str(&out)
        .map_err(|e| anyhow!("parse Anthropic response: {} — raw: {}", e,
            truncate_for_tokens(&out, 500)))?;
    // Check for {"type":"error",...} envelope.
    if v.get("type").and_then(|t| t.as_str()) == Some("error") {
        let msg = v.pointer("/error/message").and_then(|x| x.as_str())
            .unwrap_or("unknown");
        bail!("Anthropic API error: {}", msg);
    }
    let text = v.pointer("/content/0/text").and_then(|x| x.as_str())
        .ok_or_else(|| anyhow!("missing content[0].text в Anthropic response"))?
        .to_string();
    let in_tok = v.pointer("/usage/input_tokens").and_then(|x| x.as_u64())
        .unwrap_or(0) as u32;
    let out_tok = v.pointer("/usage/output_tokens").and_then(|x| x.as_u64())
        .unwrap_or(0) as u32;
    Ok(AiResponse {
        text,
        tokens_used: Some(in_tok + out_tok),
        model: cfg.model.clone(),
        provider: "anthropic".to_string(),
    })
}

fn call_openai(cfg: &AiConfig, prompt: &str, dry_run: bool) -> Result<AiResponse> {
    let body = json!({
        "model": cfg.model,
        "max_tokens": cfg.max_tokens,
        "messages": [{ "role": "user", "content": prompt }],
    });
    if dry_run {
        return Ok(AiResponse {
            text: format!("DRY RUN — would POST к https://api.openai.com/v1/chat/completions:\n{}",
                serde_json::to_string_pretty(&body)?),
            tokens_used: None,
            model: cfg.model.clone(),
            provider: "openai".to_string(),
        });
    }
    let body_str = serde_json::to_string(&body)?;
    let auth = format!("Bearer {}", cfg.api_key);
    let out = curl_post("https://api.openai.com/v1/chat/completions", &body_str, &[
        ("Authorization", &auth),
        ("Content-Type", "application/json"),
    ])?;
    let v: Value = serde_json::from_str(&out)
        .map_err(|e| anyhow!("parse OpenAI response: {} — raw: {}", e,
            truncate_for_tokens(&out, 500)))?;
    if let Some(err) = v.get("error") {
        let msg = err.get("message").and_then(|x| x.as_str()).unwrap_or("unknown");
        bail!("OpenAI API error: {}", msg);
    }
    let text = v.pointer("/choices/0/message/content").and_then(|x| x.as_str())
        .ok_or_else(|| anyhow!("missing choices[0].message.content в OpenAI response"))?
        .to_string();
    let tok = v.pointer("/usage/total_tokens").and_then(|x| x.as_u64())
        .map(|n| n as u32);
    Ok(AiResponse {
        text, tokens_used: tok,
        model: cfg.model.clone(),
        provider: "openai".to_string(),
    })
}

/// POST `body` к `url` with headers via system curl; returns stdout.
/// Adds retry-on-rate-limit (HTTP 429) up to 3 times w/ exponential backoff.
fn curl_post(url: &str, body: &str, headers: &[(&str, &str)]) -> Result<String> {
    for attempt in 0..3u32 {
        let mut cmd = Command::new("curl");
        cmd.arg("-sS")              // silent + show errors
           .arg("--fail-with-body") // non-2xx → exit non-zero, body still printed
           .arg("--connect-timeout").arg("15")
           .arg("--max-time").arg("120")
           .arg("-X").arg("POST")
           .arg(url);
        for (k, v) in headers {
            cmd.arg("-H").arg(format!("{}: {}", k, v));
        }
        // Pass body via stdin (avoid command-line length limits + secret leak).
        cmd.arg("--data-binary").arg("@-");
        cmd.stdin(std::process::Stdio::piped());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());
        let mut child = cmd.spawn()
            .map_err(|e| anyhow!("spawn curl: {} (install curl?)", e))?;
        {
            use std::io::Write;
            let stdin = child.stdin.as_mut()
                .ok_or_else(|| anyhow!("curl stdin unavailable"))?;
            stdin.write_all(body.as_bytes())
                .map_err(|e| anyhow!("write to curl stdin: {}", e))?;
        }
        let out = child.wait_with_output()
            .map_err(|e| anyhow!("wait curl: {}", e))?;
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        let stderr = String::from_utf8_lossy(&out.stderr).to_string();
        if out.status.success() {
            return Ok(stdout);
        }
        // 429 detection: stderr contains "HTTP/... 429" OR stdout has type:error rate_limit.
        let is_rate = stderr.contains(" 429") || stdout.contains("rate_limit");
        if is_rate && attempt < 2 {
            let backoff_ms = 1000u64 * (1u64 << attempt);  // 1s, 2s
            eprintln!("ai: rate-limited (attempt {}), backing off {}ms",
                attempt + 1, backoff_ms);
            std::thread::sleep(std::time::Duration::from_millis(backoff_ms));
            continue;
        }
        bail!("curl POST {} failed (status {}): {}\nstderr: {}",
            url, out.status, truncate_for_tokens(&stdout, 500), stderr);
    }
    bail!("curl POST {} failed after retries", url)
}

/// Render terminal-formatted AI block.
pub fn render_terminal(r: &AiResponse) -> String {
    let mut s = String::new();
    s.push_str("\n═══ AI interpretation ═══\n");
    s.push_str(&format!("Provider: {}  Model: {}\n", r.provider, r.model));
    if let Some(t) = r.tokens_used {
        s.push_str(&format!("Tokens: {}\n", t));
    }
    s.push_str("\n");
    s.push_str(r.text.trim());
    s.push_str("\n═════════════════════════\n");
    s
}

/// Append AI block в markdown form (suitable для PR comment).
pub fn render_markdown(r: &AiResponse) -> String {
    let mut s = String::new();
    s.push_str("\n## AI interpretation\n\n");
    s.push_str(&format!("*Provider: `{}` • Model: `{}`",
        r.provider, r.model));
    if let Some(t) = r.tokens_used {
        s.push_str(&format!(" • Tokens: {}", t));
    }
    s.push_str("*\n\n");
    s.push_str(r.text.trim());
    s.push_str("\n");
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_parses() {
        assert!(matches!(Provider::parse("anthropic").unwrap(), Provider::Anthropic));
        assert!(matches!(Provider::parse("Claude").unwrap(),    Provider::Anthropic));
        assert!(matches!(Provider::parse("openai").unwrap(),    Provider::OpenAi));
        assert!(matches!(Provider::parse("GPT").unwrap(),       Provider::OpenAi));
        assert!(Provider::parse("bogus").is_err());
    }

    #[test]
    fn truncate_preserves_full_lines() {
        let s = "alpha\nbeta\ngamma\ndelta\n";
        let out = truncate_for_tokens(s, 12);
        assert!(out.ends_with("\n"));
        assert!(!out.contains("delta"));
        assert!(out.contains("alpha"));
    }

    #[test]
    fn prompt_builder_includes_all_sections() {
        let rows = vec![DiffRow {
            name: "bench_x".to_string(),
            baseline_median_ns: Some(100.0),
            new_median_ns: Some(150.0),
            delta_pct: Some(50.0),
            p_value: Some(0.001),
            n_baseline: 30,
            n_new: 30,
        }];
        let pb = PromptBuilder {
            diff_rows: &rows,
            git_diff: Some("diff --git a/x.nv b/x.nv\n+ extra line\n".to_string()),
            git_commits: Some("abc123 refactor: change loop bounds".to_string()),
            note: Some("CI runner: linux-perf-01"),
        };
        let p = pb.build();
        assert!(p.contains("<bench_diff_table>"));
        assert!(p.contains("bench_x"));
        assert!(p.contains("+50.00%"));
        assert!(p.contains("<git_diff>"));
        assert!(p.contains("<git_commits>"));
        assert!(p.contains("<additional_note>"));
        assert!(p.contains("CI runner"));
    }

    #[test]
    fn dry_run_returns_request_body() {
        let cfg = AiConfig {
            provider: Provider::Anthropic,
            model: "claude-x".to_string(),
            api_key: "sk-test".to_string(),
            max_tokens: 100,
            include_git_diff: false,
            include_commits: false,
            max_commits: 0,
        };
        let r = call_api(&cfg, "hello", true).unwrap();
        assert!(r.text.contains("DRY RUN"));
        assert!(r.text.contains("claude-x"));
        assert!(r.text.contains("hello"));
        // dry-run should NOT leak api_key.
        assert!(!r.text.contains("sk-test"));
    }

    #[test]
    fn dry_run_openai_too() {
        let cfg = AiConfig {
            provider: Provider::OpenAi,
            model: "gpt-x".to_string(),
            api_key: "sk-y".to_string(),
            max_tokens: 50,
            include_git_diff: false,
            include_commits: false,
            max_commits: 0,
        };
        let r = call_api(&cfg, "world", true).unwrap();
        assert!(r.text.contains("DRY RUN"));
        assert_eq!(r.provider, "openai");
    }

    // ── F.4 additional positive tests ───────────────────────────────

    #[test]
    fn provider_default_models() {
        assert_eq!(Provider::Anthropic.default_model(), "claude-opus-4-7");
        assert_eq!(Provider::OpenAi.default_model(),    "gpt-4o-mini");
    }

    #[test]
    fn prompt_builder_minimal_no_optional_sections() {
        // Только diff_rows — no git diff, no commits, no note.
        let rows = vec![DiffRow {
            name: "x".to_string(),
            baseline_median_ns: Some(50.0),
            new_median_ns: Some(50.0),
            delta_pct: Some(0.0),
            p_value: Some(0.5),
            n_baseline: 10,
            n_new: 10,
        }];
        let pb = PromptBuilder {
            diff_rows: &rows,
            git_diff: None,
            git_commits: None,
            note: None,
        };
        let p = pb.build();
        // Diff table должна присутствовать всегда.
        assert!(p.contains("<bench_diff_table>"));
        assert!(p.contains("x"));
        // Optional tags NOT present.
        assert!(!p.contains("<git_diff>"));
        assert!(!p.contains("<git_commits>"));
        assert!(!p.contains("<additional_note>"));
    }

    #[test]
    fn prompt_builder_empty_rows_still_valid() {
        let pb = PromptBuilder {
            diff_rows: &[],
            git_diff: None, git_commits: None, note: None,
        };
        let p = pb.build();
        // Header row (column names) present even if 0 data rows.
        assert!(p.contains("baseline_ns"));
        assert!(p.contains("delta_pct"));
    }

    #[test]
    fn truncate_for_tokens_does_not_grow_input() {
        let s = "short\n";
        let out = truncate_for_tokens(s, 10_000);
        assert_eq!(out, s);  // ниже limit → identical.
    }

    #[test]
    fn collect_git_context_returns_none_when_shas_missing() {
        let tmp = std::env::temp_dir();
        let (d, c) = collect_git_context(&tmp, None, Some("abc"), 5);
        assert!(d.is_none() && c.is_none());
        let (d, c) = collect_git_context(&tmp, Some("abc"), None, 5);
        assert!(d.is_none() && c.is_none());
        let (d, c) = collect_git_context(&tmp, Some(""), Some(""), 5);
        assert!(d.is_none() && c.is_none());
    }

    #[test]
    fn render_terminal_contains_tokens_field_when_present() {
        let r = AiResponse {
            text: "body text".to_string(),
            tokens_used: Some(1234),
            model: "M".to_string(),
            provider: "P".to_string(),
        };
        let s = render_terminal(&r);
        assert!(s.contains("body text"));
        assert!(s.contains("Tokens: 1234"));
        assert!(s.contains("AI interpretation"));
    }

    #[test]
    fn render_markdown_skips_tokens_field_when_none() {
        let r = AiResponse {
            text: "x".to_string(),
            tokens_used: None,
            model: "m".to_string(),
            provider: "p".to_string(),
        };
        let s = render_markdown(&r);
        assert!(s.contains("## AI interpretation"));
        assert!(!s.contains("Tokens:"));
    }

    // ── F.4 additional negative tests ───────────────────────────────

    #[test]
    fn provider_parse_rejects_garbage() {
        assert!(Provider::parse("").is_err());
        assert!(Provider::parse("anthropicx").is_err());
        assert!(Provider::parse("foo bar").is_err());
    }

    #[test]
    fn dry_run_response_does_not_leak_api_key_either_provider() {
        for prov in [Provider::Anthropic, Provider::OpenAi] {
            let cfg = AiConfig {
                provider: prov,
                model: "m".to_string(),
                api_key: "sk-VERY-SECRET-DO-NOT-LEAK".to_string(),
                max_tokens: 10,
                include_git_diff: false,
                include_commits: false,
                max_commits: 0,
            };
            let r = call_api(&cfg, "p", true).unwrap();
            assert!(!r.text.contains("VERY-SECRET"),
                "dry-run output leaked api_key: {}", r.text);
        }
    }

    #[test]
    fn truncate_for_tokens_zero_returns_empty() {
        // max=0 boundary: no lines fit → empty.
        let out = truncate_for_tokens("a\nb\nc\n", 0);
        assert_eq!(out, "");
    }

    #[test]
    fn fmt_opt_handles_none() {
        // Smoke: helpers should produce "-" placeholder для None.
        assert_eq!(fmt_opt(&None),         "-");
        assert_eq!(fmt_opt_pct(&None),     "-");
        assert_eq!(fmt_opt_p(&None),       "-");
        // Some values: formatted.
        assert_eq!(fmt_opt(&Some(1.5)),    "1.5");
        assert!(fmt_opt_pct(&Some(1.5)).starts_with("+1.50"));
        assert_eq!(fmt_opt_p(&Some(0.001)),"0.0010");
    }
}
