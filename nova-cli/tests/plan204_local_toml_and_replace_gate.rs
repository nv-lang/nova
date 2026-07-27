//! Plan 204 дофикс №2 (owner correction) — CLI-level integration tests for
//! two policy gates the compiler-codegen unit/e2e suites can't reach (they
//! live in the `nova` binary crate, not `nova_codegen`):
//!
//!   1. `nova build` hard-errors (`E_REPLACE_IN_MANIFEST`) on a committed
//!      `[replace]` section — fails FAST, before any toolchain/codegen
//!      work (the check runs right after manifest parse).
//!   2. `nova add <name> --path DIR` refuses to write a bare `path` into
//!      `[dependencies]` when `DIR` escapes the current git repository
//!      (not clone-safe) — unless `--allow-external-path` is given.
//!      In-repo `--path` targets are unaffected (no gating).
//!
//! Fixtures live under the OS temp dir (NOT inside this repo's own tree) —
//! real `git init` repos are needed to exercise the repo-boundary check
//! (`manifest::git_repo_root`), and using temp dirs avoids any ambiguity
//! about which `nova.toml` `find_repo_root()` would discover.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn unique(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "nova_p204_cli_{}_{}_{}",
        tag,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

fn write_file(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create_dir_all");
    }
    fs::write(path, content).expect("write fixture file");
}

fn git(args: &[&str], cwd: &Path) {
    let out = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("run git");
    assert!(
        out.status.success(),
        "git {:?} (cwd={}) failed: {}",
        args, cwd.display(), String::from_utf8_lossy(&out.stderr),
    );
}

fn init_repo(dir: &Path) {
    fs::create_dir_all(dir).unwrap();
    git(&["init", "--quiet", "."], dir);
    git(&["config", "user.email", "t@t"], dir);
    git(&["config", "user.name", "t"], dir);
}

fn nova(cwd: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_nova"))
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("failed to spawn `nova`")
}

/// `nova build` on a package whose COMMITTED `nova.toml` declares
/// `[replace]` — must hard-fail with `E_REPLACE_IN_MANIFEST`, BEFORE any
/// git materialization / toolchain work (the dep-declared git URL is
/// deliberately bogus — if the build got that far it would fail for the
/// WRONG reason).
#[test]
fn nova_build_hard_errors_on_committed_replace() {
    let dir = unique("committed_replace_build");
    write_file(
        &dir.join("nova.toml"),
        "[package]\nname = \"app\"\n[lib]\nsrc = \".\"\n\
         [dependencies]\ntls = { git = \"https://example.invalid/nonexistent-tls\", version = \"0.1\" }\n\
         [replace]\ntls = { path = \"../nova-tls\" }\n",
    );
    write_file(&dir.join("app.nv"), "module app\n\nfn main() -> int => 0\n");

    let out = nova(&dir, &["build", "app.nv"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "build must fail; stderr: {}", stderr);
    assert!(
        stderr.contains("E_REPLACE_IN_MANIFEST"),
        "expected E_REPLACE_IN_MANIFEST in stderr, got: {}", stderr,
    );
    assert!(
        // Plan 233 §2а: hint renamed nova.local.toml -> nova.override.toml.
        stderr.contains("nova.override.toml"),
        "expected a nova.override.toml hint, got: {}", stderr,
    );

    fs::remove_dir_all(&dir).ok();
}

/// `nova add extlib --path <DIR>` where `DIR` is OUTSIDE the current git
/// repo — refused (no `--allow-external-path`): nova.toml unchanged, stderr
/// carries the git+version / nova.override.toml recipe hint.
#[test]
fn add_external_path_without_flag_is_refused_with_hint() {
    let repo_app = unique("add_ext_app");
    init_repo(&repo_app);
    write_file(&repo_app.join("nova.toml"), "[package]\nname = \"app\"\n[lib]\nsrc = \".\"\n");
    let toml_before = fs::read_to_string(repo_app.join("nova.toml")).unwrap();

    // A DIFFERENT repo — genuinely external relative to `repo_app`.
    let repo_ext = unique("add_ext_lib");
    init_repo(&repo_ext);
    write_file(&repo_ext.join("nova.toml"), "[package]\nname = \"extlib\"\n[lib]\nsrc = \".\"\n");

    let rel = pathdiff(&repo_app, &repo_ext);
    let out = nova(&repo_app, &["add", "extlib", "--path", &rel]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "add must refuse; stderr: {}", stderr);
    assert!(stderr.contains("--allow-external-path"), "stderr: {}", stderr);
    // Plan 233 §2а: hint renamed nova.local.toml -> nova.override.toml.
    assert!(stderr.contains("nova.override.toml"), "stderr: {}", stderr);

    let toml_after = fs::read_to_string(repo_app.join("nova.toml")).unwrap();
    assert_eq!(toml_before, toml_after, "nova.toml must be unchanged on refusal");

    fs::remove_dir_all(&repo_app).ok();
    fs::remove_dir_all(&repo_ext).ok();
}

/// Same external-repo scenario, but WITH `--allow-external-path` — old
/// behavior (bare `path` written to `[dependencies]`) still available.
#[test]
fn add_external_path_with_flag_succeeds() {
    let repo_app = unique("add_ext_ok_app");
    init_repo(&repo_app);
    write_file(&repo_app.join("nova.toml"), "[package]\nname = \"app\"\n[lib]\nsrc = \".\"\n");

    let repo_ext = unique("add_ext_ok_lib");
    init_repo(&repo_ext);
    write_file(&repo_ext.join("nova.toml"), "[package]\nname = \"extlib\"\n[lib]\nsrc = \".\"\n");

    let rel = pathdiff(&repo_app, &repo_ext);
    let out = nova(&repo_app, &["add", "extlib", "--path", &rel, "--allow-external-path"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "add --allow-external-path must succeed; stderr: {}", stderr);

    let toml_after = fs::read_to_string(repo_app.join("nova.toml")).unwrap();
    assert!(toml_after.contains("extlib"), "toml: {}", toml_after);

    fs::remove_dir_all(&repo_app).ok();
    fs::remove_dir_all(&repo_ext).ok();
}

/// `--path` target INSIDE the same git repo (nested sibling package) —
/// succeeds WITHOUT `--allow-external-path` (no gating for clone-safe
/// in-repo paths).
#[test]
fn add_in_repo_path_succeeds_without_flag() {
    let repo = unique("add_inrepo");
    init_repo(&repo);
    let app_dir = repo.join("app");
    let lib_dir = repo.join("libint");
    write_file(&app_dir.join("nova.toml"), "[package]\nname = \"app\"\n[lib]\nsrc = \".\"\n");
    write_file(&lib_dir.join("nova.toml"), "[package]\nname = \"libint\"\n[lib]\nsrc = \".\"\n");

    let out = nova(&app_dir, &["add", "libint", "--path", "../libint"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "in-repo add must succeed without the flag; stderr: {}", stderr);

    let toml_after = fs::read_to_string(app_dir.join("nova.toml")).unwrap();
    assert!(toml_after.contains("libint"), "toml: {}", toml_after);

    fs::remove_dir_all(&repo).ok();
}

/// Naive relative-path helper (siblings under the same OS temp root).
fn pathdiff(from: &Path, to: &Path) -> String {
    let from_parent = from.parent().unwrap();
    let to_name = to.file_name().unwrap().to_string_lossy();
    if from_parent == to.parent().unwrap() {
        format!("../{}", to_name)
    } else {
        to.to_string_lossy().replace('\\', "/")
    }
}
