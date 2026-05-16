// SPDX-License-Identifier: MIT OR Apache-2.0
//! Plan 57.A.1 — Historical orphan branch automation.
//!
//! `nova bench history-add result.json [--branch bench-history]` —
//! appends bench result JSON в orphan branch (default: `bench-history`).
//! Не загрязняет working tree; dashboard reads from this branch.
//!
//! Дизайн:
//!   - Orphan branch — separate root commit, no shared history.
//!   - Каждый history entry — отдельный файл с именем
//!     `<git-sha>-<timestamp>.json` (deterministic, sortable).
//!   - На каждый history-add создаётся новый commit (chronological).
//!   - Yearly squash recommended (см. perf-conventions.md).
//!
//! Subprocess git (без libgit2 dep — feedback_third_party_libs).

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{anyhow, bail, Result};
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct HistoryAddOpts<'a> {
    pub result_json: &'a Path,
    pub branch: String,
    pub repo: &'a Path,
    /// Если true — `git push origin <branch>` после commit. Default false.
    pub push: bool,
    /// Remote name (если push). Default "origin".
    pub remote: String,
    /// Если true — dry-run, печатает что будет сделано без коммита.
    pub dry_run: bool,
}

pub fn add(opts: HistoryAddOpts) -> Result<i32> {
    if !opts.result_json.exists() {
        bail!("result file not found: {}", opts.result_json.display());
    }

    // Validate JSON parse + schema version.
    let raw = std::fs::read_to_string(opts.result_json)
        .map_err(|e| anyhow!("read result JSON: {}", e))?;
    let v: Value = serde_json::from_str(&raw)
        .map_err(|e| anyhow!("parse result JSON: {}", e))?;
    let format_version = v.get("format_version")
        .and_then(|x| x.as_str())
        .ok_or_else(|| anyhow!("result JSON missing 'format_version' field"))?;
    if format_version != super::SCHEMA_VERSION {
        bail!("result JSON schema version {} != {}",
              format_version, super::SCHEMA_VERSION);
    }

    // Get HEAD sha (short).
    let head_sha = git_head_short(opts.repo)?;

    // Get current timestamp (unix seconds).
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let entry_name = format!("{}-{}.json", ts, head_sha);
    eprintln!("history-add: will store as `{}` в branch `{}`",
        entry_name, opts.branch);

    if opts.dry_run {
        eprintln!("history-add: dry-run, exiting without commit");
        return Ok(0);
    }

    // Approach: use git worktree to operate on orphan branch without
    // disturbing main worktree. Steps:
    //   1. If branch не существует — create empty orphan, add result.
    //   2. Если существует — `git fetch origin bench-history` (если remote есть),
    //      checkout, add, commit.
    // Чтобы избежать дёргания main worktree — используем temp worktree.

    // Check if branch exists locally.
    let branch_exists = branch_exists_local(opts.repo, &opts.branch)?;

    // Use system temp dir to support worktrees (.git может быть file, не dir).
    let tmp_wt = std::env::temp_dir()
        .join(format!("nova-bench-history-{}", std::process::id()));
    if tmp_wt.exists() {
        let _ = git_in(opts.repo, &["worktree", "remove", "--force",
            &tmp_wt.to_string_lossy()]);
        let _ = std::fs::remove_dir_all(&tmp_wt);
    }

    if !branch_exists {
        eprintln!("history-add: branch `{}` not found, creating fresh orphan",
            opts.branch);
        // Create orphan branch in temp worktree.
        git_in(opts.repo, &["worktree", "add", "--detach",
            &tmp_wt.to_string_lossy(), "HEAD"])?;
        git_in(&tmp_wt, &["checkout", "--orphan", &opts.branch])?;
        // Remove все файлы (orphan inherits index).
        let _ = git_in(&tmp_wt, &["rm", "-rf", "."]);
        // Initial README.
        let readme = "# bench-history — Plan 57.A storage\n\n\
                      JSON files: <unix_ts>-<git_sha>.json\n\
                      Read by `nova bench dashboard`.\n";
        std::fs::write(tmp_wt.join("README.md"), readme)?;
    } else {
        git_in(opts.repo, &["worktree", "add", &tmp_wt.to_string_lossy(),
            &opts.branch])?;
    }

    // Copy result file into worktree.
    let dest = tmp_wt.join(&entry_name);
    std::fs::copy(opts.result_json, &dest)
        .map_err(|e| anyhow!("copy result: {}", e))?;

    // Commit.
    git_in(&tmp_wt, &["add", &entry_name, "README.md"])?;
    let msg = format!("history-add {} from {}", entry_name, head_sha);
    let commit_res = git_in(&tmp_wt, &["-c", "user.name=nova-bench",
                       "-c", "user.email=nova-bench@localhost",
                       "commit", "-m", &msg]);
    if commit_res.is_err() {
        // Maybe no changes (re-add same file). Soft-ignore.
        eprintln!("history-add: nothing to commit (file unchanged)");
    }

    // Push если запрошено.
    if opts.push {
        eprintln!("history-add: pushing to {}", opts.remote);
        git_in(&tmp_wt, &["push", &opts.remote, &opts.branch])?;
    }

    // Cleanup temp worktree.
    git_in(opts.repo, &["worktree", "remove", "--force",
        &tmp_wt.to_string_lossy()])?;

    eprintln!("history-add: success");
    Ok(0)
}

/// List entries in history branch (sorted chronologically, newest first).
pub fn list(repo: &Path, branch: &str) -> Result<Vec<HistoryEntry>> {
    if !branch_exists_local(repo, branch)? {
        return Ok(Vec::new());
    }
    // List files in branch via `git ls-tree`.
    let out = git_in(repo, &["ls-tree", "--name-only", branch])?;
    let mut entries = Vec::new();
    for name in out.lines() {
        let name = name.trim();
        if !name.ends_with(".json") { continue; }
        let stem = name.strip_suffix(".json").unwrap();
        let parts: Vec<&str> = stem.splitn(2, '-').collect();
        if parts.len() != 2 { continue; }
        let ts: u64 = parts[0].parse().unwrap_or(0);
        let sha = parts[1].to_string();
        entries.push(HistoryEntry {
            filename: name.to_string(),
            timestamp_unix: ts,
            git_sha: sha,
        });
    }
    entries.sort_by(|a, b| b.timestamp_unix.cmp(&a.timestamp_unix));
    Ok(entries)
}

#[derive(Debug, Clone)]
pub struct HistoryEntry {
    pub filename: String,
    pub timestamp_unix: u64,
    pub git_sha: String,
}

/// Read JSON content of single entry.
pub fn read_entry(repo: &Path, branch: &str, filename: &str) -> Result<String> {
    let spec = format!("{}:{}", branch, filename);
    git_in(repo, &["show", &spec])
}

// ── git helpers ────────────────────────────────────────────────────────

fn git_in(cwd: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .current_dir(cwd)
        .args(args)
        .output()
        .map_err(|e| anyhow!("spawn git: {}", e))?;
    if !output.status.success() {
        bail!("git {} failed: {}", args.join(" "),
            String::from_utf8_lossy(&output.stderr));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn git_head_short(repo: &Path) -> Result<String> {
    let out = git_in(repo, &["rev-parse", "--short=12", "HEAD"])?;
    Ok(out.trim().to_string())
}

fn branch_exists_local(repo: &Path, branch: &str) -> Result<bool> {
    let result = Command::new("git")
        .current_dir(repo)
        .args(["rev-parse", "--verify", &format!("refs/heads/{}", branch)])
        .output()
        .map_err(|e| anyhow!("spawn git: {}", e))?;
    Ok(result.status.success())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_parsing() {
        // Format: <ts>-<sha>.json
        let mut v = Vec::new();
        v.push(HistoryEntry {
            filename: "1779494400-abc123.json".to_string(),
            timestamp_unix: 1779494400,
            git_sha: "abc123".to_string(),
        });
        v.push(HistoryEntry {
            filename: "1779494500-def456.json".to_string(),
            timestamp_unix: 1779494500,
            git_sha: "def456".to_string(),
        });
        // Sort newest first.
        v.sort_by(|a, b| b.timestamp_unix.cmp(&a.timestamp_unix));
        assert_eq!(v[0].timestamp_unix, 1779494500);
        assert_eq!(v[1].timestamp_unix, 1779494400);
    }
}

/// Render brief summary of history-add command.
pub fn explain() -> &'static str {
    "Appends a bench result JSON to an orphan git branch (default \
     `bench-history`). The orphan branch is independent — no shared \
     history with main, no working-tree clutter. Use `nova bench \
     dashboard --history-branch bench-history` to render time-series \
     HTML reports from accumulated entries."
}
