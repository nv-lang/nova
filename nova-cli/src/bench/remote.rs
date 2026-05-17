// SPDX-License-Identifier: MIT OR Apache-2.0
//! Plan 57.F.1 — SSH-based distributed bench coordination.
//!
//! Production-grade orchestrator: parallel bench runs across N remote
//! machines, results gathered + aggregated. No external dependencies
//! beyond system `ssh`/`scp` binaries (standard on Linux/macOS/Windows
//! 10+, no Rust crates).
//!
//! Config: `~/.nova-bench-remotes.toml` (minimal TOML inline parser
//! как `bench.toml`).
//!
//! Workflow:
//!   1. `nova bench remote list` — print configured remotes.
//!   2. `nova bench remote ping <name>` — ssh-based health check.
//!   3. `nova bench remote run <bench-file> --remotes A,B,C
//!                              --gather-into ./remote-results/`
//!      Параллельно: ssh + scp result.json back per host.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{anyhow, bail, Result};

pub const DEFAULT_CONFIG_PATH: &str = ".nova-bench-remotes.toml";

#[derive(Debug, Clone)]
pub struct RemoteConfig {
    pub name: String,
    pub host: String,
    pub user: String,
    pub repo: String,
    pub runner_id: String,
    pub ssh_key: Option<String>,
    pub ssh_port: Option<u16>,
}

#[derive(Debug, Clone, Default)]
pub struct RemotesFile {
    pub remotes: Vec<RemoteConfig>,
    pub parse_errors: Vec<String>,
}

impl RemotesFile {
    /// Resolve config path: $NOVA_BENCH_REMOTES env → arg → default ~/.
    pub fn resolve_path(explicit: Option<&Path>) -> Option<PathBuf> {
        if let Some(p) = explicit { return Some(p.to_path_buf()); }
        if let Ok(env_p) = std::env::var("NOVA_BENCH_REMOTES") {
            return Some(PathBuf::from(env_p));
        }
        // ~/.nova-bench-remotes.toml — cross-platform home detection.
        if let Some(home) = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE")) {
            return Some(PathBuf::from(home).join(DEFAULT_CONFIG_PATH));
        }
        None
    }

    pub fn load_or_default(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(s) => Self::parse(&s),
            Err(_) => Self::default(),
        }
    }

    /// Минималистичный parser для `[remote.NAME] key = "value"` sections.
    /// Не поддерживает inline tables, dotted keys, или multi-line strings.
    pub fn parse(input: &str) -> Self {
        let mut out = Self::default();
        let mut current: Option<RemoteConfig> = None;
        for (lineno, raw_line) in input.lines().enumerate() {
            let line = raw_line.split('#').next().unwrap_or("").trim();
            if line.is_empty() { continue; }
            if let Some(rest) = line.strip_prefix('[') {
                if let Some(name_str) = rest.strip_suffix(']') {
                    // Push previous.
                    if let Some(r) = current.take() {
                        out.remotes.push(r);
                    }
                    let parts: Vec<&str> = name_str.trim().split('.').collect();
                    if parts.len() == 2 && parts[0] == "remote" {
                        current = Some(RemoteConfig {
                            name: parts[1].to_string(),
                            host: String::new(),
                            user: String::new(),
                            repo: String::new(),
                            runner_id: parts[1].to_string(),  // default = name
                            ssh_key: None,
                            ssh_port: None,
                        });
                    } else {
                        current = None;
                    }
                    continue;
                }
            }
            // key = value line.
            let r = match current.as_mut() {
                Some(r) => r,
                None => continue,
            };
            if let Some(eq) = line.find('=') {
                let key = line[..eq].trim();
                let val_raw = line[eq + 1..].trim();
                let unquoted = if val_raw.len() >= 2
                    && val_raw.starts_with('"') && val_raw.ends_with('"') {
                    val_raw[1..val_raw.len() - 1].to_string()
                } else {
                    val_raw.to_string()
                };
                match key {
                    "host"      => r.host = unquoted,
                    "user"      => r.user = unquoted,
                    "repo"      => r.repo = unquoted,
                    "runner_id" => r.runner_id = unquoted,
                    "ssh_key"   => r.ssh_key = Some(unquoted),
                    "ssh_port"  => match unquoted.parse() {
                        Ok(n) => r.ssh_port = Some(n),
                        Err(_) => out.parse_errors.push(
                            format!("line {}: invalid ssh_port: {}",
                                lineno + 1, unquoted)),
                    },
                    _ => out.parse_errors.push(
                        format!("line {}: unknown key `{}`", lineno + 1, key)),
                }
            }
        }
        if let Some(r) = current.take() { out.remotes.push(r); }
        // Validation: filter incomplete entries.
        out.remotes.retain(|r| {
            if r.host.is_empty() || r.user.is_empty() || r.repo.is_empty() {
                out.parse_errors.push(
                    format!("remote `{}` missing required field (host/user/repo)",
                        r.name));
                false
            } else { true }
        });
        out
    }

    pub fn find(&self, name: &str) -> Option<&RemoteConfig> {
        self.remotes.iter().find(|r| r.name == name)
    }
}

impl RemoteConfig {
    /// Build ssh command args: `ssh [-p port] [-i key] user@host`.
    fn ssh_base_args(&self) -> Vec<String> {
        let mut args = vec!["-o".to_string(), "BatchMode=yes".to_string(),
                            "-o".to_string(), "StrictHostKeyChecking=accept-new".to_string()];
        if let Some(p) = self.ssh_port {
            args.push("-p".to_string());
            args.push(p.to_string());
        }
        if let Some(k) = &self.ssh_key {
            args.push("-i".to_string());
            args.push(k.clone());
        }
        args.push(format!("{}@{}", self.user, self.host));
        args
    }

    /// Ping check: ssh + echo. Returns Ok if reachable.
    pub fn ping(&self) -> Result<()> {
        let mut cmd = Command::new("ssh");
        for a in self.ssh_base_args() { cmd.arg(a); }
        cmd.arg("echo pong");
        let out = cmd.output()
            .map_err(|e| anyhow!("spawn ssh: {}", e))?;
        if !out.status.success() {
            bail!("ssh {}@{} failed: {}", self.user, self.host,
                String::from_utf8_lossy(&out.stderr));
        }
        let stdout = String::from_utf8_lossy(&out.stdout);
        if !stdout.contains("pong") {
            bail!("ping response unexpected: {}", stdout);
        }
        Ok(())
    }

    /// Run bench remotely:
    ///   1. ssh "cd repo && git checkout <sha> && nova-cli/target/release/nova
    ///       bench run <bench> --out /tmp/r.json"
    ///   2. scp host:/tmp/r.json local_out
    pub fn run_bench(&self, git_sha: Option<&str>, bench_path: &str,
                     local_out: &Path) -> Result<()> {
        let remote_out = format!("/tmp/nova-bench-{}-{}.json",
            self.runner_id, std::process::id());

        // 1. Build remote command.
        let mut remote_cmd = format!("cd {}", shell_quote(&self.repo));
        if let Some(sha) = git_sha {
            remote_cmd.push_str(&format!(" && git fetch && git checkout {}",
                shell_quote(sha)));
        }
        remote_cmd.push_str(&format!(
            " && NOVA_BENCH_RUNNER_ID={} nova-cli/target/release/nova bench run {} \
               --gc malloc --mode release --out {}",
            shell_quote(&self.runner_id),
            shell_quote(bench_path),
            shell_quote(&remote_out),
        ));

        let mut cmd = Command::new("ssh");
        for a in self.ssh_base_args() { cmd.arg(a); }
        cmd.arg(&remote_cmd);
        let out = cmd.output()
            .map_err(|e| anyhow!("spawn ssh для bench: {}", e))?;
        if !out.status.success() {
            bail!("remote bench run failed на {}: {}", self.name,
                String::from_utf8_lossy(&out.stderr));
        }

        // 2. scp result back.
        let mut scp = Command::new("scp");
        scp.arg("-o").arg("BatchMode=yes");
        if let Some(p) = self.ssh_port {
            scp.arg("-P").arg(p.to_string());
        }
        if let Some(k) = &self.ssh_key {
            scp.arg("-i").arg(k);
        }
        scp.arg(format!("{}@{}:{}", self.user, self.host, remote_out));
        scp.arg(local_out);
        let scp_out = scp.output()
            .map_err(|e| anyhow!("spawn scp: {}", e))?;
        if !scp_out.status.success() {
            bail!("scp from {} failed: {}", self.name,
                String::from_utf8_lossy(&scp_out.stderr));
        }
        Ok(())
    }
}

/// Minimal shell-safe quoting для bash args. Single-quote wrap.
fn shell_quote(s: &str) -> String {
    if s.chars().all(|c| c.is_ascii_alphanumeric() || c == '/' || c == '_' || c == '-' || c == '.') {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

/// Orchestrate parallel bench runs across remotes. Returns per-remote
/// (name, Result<local_json_path>).
pub fn run_distributed(remotes: &[&RemoteConfig], git_sha: Option<&str>,
                       bench_path: &str, gather_dir: &Path)
    -> Vec<(String, Result<PathBuf>)>
{
    std::fs::create_dir_all(gather_dir).ok();
    let mut handles = Vec::new();
    for r in remotes {
        let r_cloned = (*r).clone();
        let bench_path_owned = bench_path.to_string();
        let gather = gather_dir.to_path_buf();
        let sha_owned = git_sha.map(|s| s.to_string());
        let h = std::thread::spawn(move || {
            let out = gather.join(format!("{}.json", r_cloned.runner_id));
            let res = r_cloned.run_bench(sha_owned.as_deref(),
                &bench_path_owned, &out)
                .map(|_| out);
            (r_cloned.name.clone(), res)
        });
        handles.push(h);
    }
    let mut results = Vec::with_capacity(handles.len());
    for h in handles {
        match h.join() {
            Ok(r) => results.push(r),
            Err(_) => results.push(("?".to_string(),
                Err(anyhow!("thread join failed")))),
        }
    }
    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_basic_config() {
        let input = r#"
[remote.linux-xeon]
host = "perf-1.example.com"
user = "bench"
repo = "/home/bench/nova-lang"
runner_id = "linux-xeon-perf"

[remote.arm-cloud]
host = "192.0.2.5"
user = "bench"
repo = "/srv/nova-lang"
ssh_port = 2222
"#;
        let f = RemotesFile::parse(input);
        assert!(f.parse_errors.is_empty(), "errors: {:?}", f.parse_errors);
        assert_eq!(f.remotes.len(), 2);
        let xeon = f.find("linux-xeon").unwrap();
        assert_eq!(xeon.host, "perf-1.example.com");
        assert_eq!(xeon.runner_id, "linux-xeon-perf");
        assert!(xeon.ssh_port.is_none());
        let arm = f.find("arm-cloud").unwrap();
        assert_eq!(arm.ssh_port, Some(2222));
        // runner_id defaults к name если не set explicitly.
        assert_eq!(arm.runner_id, "arm-cloud");
    }

    #[test]
    fn parse_rejects_incomplete() {
        let input = r#"
[remote.incomplete]
host = "x.example"
# missing user + repo
"#;
        let f = RemotesFile::parse(input);
        assert_eq!(f.remotes.len(), 0);
        assert!(!f.parse_errors.is_empty());
    }

    #[test]
    fn shell_quote_safe() {
        assert_eq!(shell_quote("simple"), "simple");
        assert_eq!(shell_quote("a/b/c.txt"), "a/b/c.txt");
        assert_eq!(shell_quote("a b"), "'a b'");
        assert_eq!(shell_quote("a'b"), "'a'\\''b'");
    }

    #[test]
    fn ssh_base_args_builds() {
        let r = RemoteConfig {
            name: "x".to_string(), host: "h".to_string(), user: "u".to_string(),
            repo: "/r".to_string(), runner_id: "x".to_string(),
            ssh_key: Some("/k".to_string()), ssh_port: Some(2222),
        };
        let args = r.ssh_base_args();
        // Verify -p 2222 + -i /k + u@h присутствуют.
        assert!(args.iter().any(|a| a == "-p"));
        assert!(args.iter().any(|a| a == "2222"));
        assert!(args.iter().any(|a| a == "-i"));
        assert!(args.iter().any(|a| a == "/k"));
        assert!(args.iter().any(|a| a == "u@h"));
    }

    // ── F.4 additional positive tests ───────────────────────────────

    #[test]
    fn parse_with_comments_and_blank_lines() {
        let input = r#"
# top-level comment
[remote.host1]
# inline section comment

host = "h1.example"  # trailing comment ignored
user = "u1"
repo = "/srv/n"
"#;
        let f = RemotesFile::parse(input);
        assert!(f.parse_errors.is_empty(), "unexpected errors: {:?}", f.parse_errors);
        assert_eq!(f.remotes.len(), 1);
        let r = f.find("host1").unwrap();
        assert_eq!(r.host, "h1.example");
        assert_eq!(r.user, "u1");
    }

    #[test]
    fn ssh_base_args_minimal_no_key_no_port() {
        let r = RemoteConfig {
            name: "m".to_string(), host: "h".to_string(), user: "u".to_string(),
            repo: "/r".to_string(), runner_id: "m".to_string(),
            ssh_key: None, ssh_port: None,
        };
        let args = r.ssh_base_args();
        // -p / -i should NOT appear when None.
        assert!(!args.iter().any(|a| a == "-p"), "args={:?}", args);
        assert!(!args.iter().any(|a| a == "-i"), "args={:?}", args);
        // user@host still present.
        assert!(args.iter().any(|a| a == "u@h"));
        // BatchMode flag always present (no interactive prompts).
        assert!(args.iter().any(|a| a == "BatchMode=yes"));
    }

    #[test]
    fn shell_quote_no_escape_needed() {
        assert_eq!(shell_quote("abc123"),    "abc123");
        assert_eq!(shell_quote("path/_-./file.json"), "path/_-./file.json");
    }

    #[test]
    fn shell_quote_preserves_value_after_split() {
        // After shell parse: 'a b' → "a b" (whole string passed as one arg).
        let q = shell_quote("hello world");
        assert!(q.starts_with('\''));
        assert!(q.ends_with('\''));
        assert!(q.contains("hello world"));
    }

    #[test]
    fn find_returns_none_for_unknown() {
        let f = RemotesFile { remotes: vec![RemoteConfig {
            name: "x".to_string(), host: "h".to_string(), user: "u".to_string(),
            repo: "/r".to_string(), runner_id: "x".to_string(),
            ssh_key: None, ssh_port: None,
        }], parse_errors: vec![] };
        assert!(f.find("y").is_none());
        assert!(f.find("x").is_some());
    }

    // ── F.4 additional negative tests ───────────────────────────────

    #[test]
    fn parse_rejects_invalid_ssh_port() {
        let input = r#"
[remote.bad]
host = "h"
user = "u"
repo = "/r"
ssh_port = "not-a-number"
"#;
        let f = RemotesFile::parse(input);
        // Remote still loaded (other fields valid), но parse_error reported.
        assert!(!f.parse_errors.is_empty(),
            "expected parse_error для invalid ssh_port");
        let r = f.find("bad").unwrap();
        assert_eq!(r.ssh_port, None);  // не set из-за parse fail
    }

    #[test]
    fn parse_warns_on_unknown_key() {
        let input = r#"
[remote.x]
host = "h"
user = "u"
repo = "/r"
weird_field = "value"
"#;
        let f = RemotesFile::parse(input);
        assert_eq!(f.remotes.len(), 1);
        assert!(f.parse_errors.iter().any(|e| e.contains("weird_field")),
            "expected parse_error mentioning unknown key: {:?}",
            f.parse_errors);
    }

    #[test]
    fn parse_rejects_section_without_name() {
        // [bogus] — не remote.NAME.
        let input = r#"
[bogus]
host = "h"
user = "u"
repo = "/r"

[remote.good]
host = "h"
user = "u"
repo = "/r"
"#;
        let f = RemotesFile::parse(input);
        // Only "good" should appear; "bogus" не recognized как remote.
        assert_eq!(f.remotes.len(), 1);
        assert_eq!(f.remotes[0].name, "good");
    }

    #[test]
    fn parse_key_value_without_section_ignored() {
        // K=V lines перед любой [section] silently dropped.
        let input = r#"
orphan = "value"
host = "h"
[remote.real]
host = "h"
user = "u"
repo = "/r"
"#;
        let f = RemotesFile::parse(input);
        assert_eq!(f.remotes.len(), 1);
        assert_eq!(f.remotes[0].host, "h");  // не overwritten orphan.host
    }

    #[test]
    fn parse_runner_id_defaults_to_name_when_unset() {
        let input = r#"
[remote.alpha]
host = "h"
user = "u"
repo = "/r"
"#;
        let f = RemotesFile::parse(input);
        let r = f.find("alpha").unwrap();
        assert_eq!(r.runner_id, "alpha");
    }
}
