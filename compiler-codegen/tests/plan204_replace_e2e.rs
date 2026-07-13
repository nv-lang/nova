//! Plan 204 — `[replace]` end-to-end + real nova-tls `file://` smoke.
//!
//! Плановая дельта поверх уже закрытых Plan 03.1/03.2 (git+semver deps,
//! backtracking-резолвер, `nova.lock`): `[replace]`-блок перекрывает
//! источник `[dependencies]`-записи для локальной разработки (go-школа).
//! Голая `path`-запись без соответствующей git+version формы — warning
//! (`manifest::manifest_warnings`), не ошибка.
//!
//! Источник для git-теста — ЛОКАЛЬНАЯ репа `nova-tls` (сосед-репозиторий,
//! `../nova-tls` относительно этого worktree) через `file://` URL — офлайн,
//! детерминированно, без сети (реальный `v0.1.0` тег уже заведён владельцем).

use nova_codegen::lockfile::{self, LockedSource};
use nova_codegen::manifest;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn git(args: &[&str], cwd: Option<&Path>) -> String {
    let mut cmd = Command::new("git");
    cmd.args(args);
    if let Some(d) = cwd {
        cmd.current_dir(d);
    }
    let out = cmd.output().expect("run git");
    assert!(
        out.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn unique(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "nova_p204_{}_{}_{}",
        tag,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

fn init_repo(dir: &Path) {
    fs::create_dir_all(dir).unwrap();
    let d = dir.to_string_lossy().to_string();
    git(&["init", "--quiet", &d], None);
    git(&["-C", &d, "config", "user.email", "t@t"], None);
    git(&["-C", &d, "config", "user.name", "t"], None);
}

fn commit_tag(dir: &Path, tag: &str) -> String {
    let d = dir.to_string_lossy().to_string();
    git(&["-C", &d, "add", "-A"], None);
    git(&["-C", &d, "commit", "--quiet", "-m", tag], None);
    git(&["-C", &d, "tag", tag], None);
    git(&["-C", &d, "rev-parse", "HEAD"], None)
}

/// `[replace]` overrides a `{ git, version }` dependency to a local `path`
/// — `nova.lock` must record the PATH source (what's actually imported
/// from), not a git/commit entry, and no network/git resolve for that dep
/// is attempted at all (offline-safe dev loop).
#[test]
fn replace_overrides_to_path_in_lock() {
    let libb = unique("libb");
    init_repo(&libb);
    fs::write(
        libb.join("nova.toml"),
        "[package]\nname = \"libb\"\nversion = \"1.0.0\"\n[lib]\nsrc = \".\"\n",
    )
    .unwrap();
    fs::write(libb.join("core.nv"), "module libb.core\n\nexport fn b() -> int => 1\n").unwrap();
    commit_tag(&libb, "v1.0.0");
    let libb_url = libb.to_string_lossy().replace('\\', "/");

    // Local sibling override — points at libb's OWN checked-out sources
    // directly (no git involved at all for this dep).
    let libb_local = unique("libb_local");
    fs::create_dir_all(&libb_local).unwrap();
    fs::write(
        libb_local.join("nova.toml"),
        "[package]\nname = \"libb\"\nversion = \"1.0.0-dev\"\n[lib]\nsrc = \".\"\n",
    )
    .unwrap();
    fs::write(libb_local.join("core.nv"), "module libb.core\n\nexport fn b() -> int => 999\n").unwrap();

    let consumer = unique("consumer");
    fs::create_dir_all(&consumer).unwrap();
    let libb_local_rel = pathdiff(&consumer, &libb_local);
    fs::write(
        consumer.join("nova.toml"),
        format!(
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n[lib]\nsrc = \".\"\n\
             [dependencies]\nlibb = {{ git = \"{}\", version = \"^1.0\" }}\n\
             [replace]\nlibb = {{ path = \"{}\" }}\n",
            libb_url, libb_local_rel,
        ),
    )
    .unwrap();

    let m = manifest::parse_manifest(&consumer.join("nova.toml"), &consumer).expect("parse");
    // No bare-path warning: the DECLARED [dependencies] form is git+version;
    // path only appears as a [replace] override.
    let warnings = manifest::manifest_warnings(&m, &consumer.join("nova.toml"));
    assert!(warnings.is_empty(), "unexpected warnings: {:?}", warnings);

    let cache_home = unique("home");
    std::env::set_var("NOVA_HOME", &cache_home);
    let res = lockfile::sync(&consumer);
    std::env::remove_var("NOVA_HOME");
    assert!(res.is_ok(), "sync with replace: {:?}", res.err());

    let lock = lockfile::load(&consumer).expect("load").expect("lock exists");
    assert_eq!(lock.packages.len(), 1);
    match &lock.packages[0].source {
        LockedSource::Path { path } => assert_eq!(path, &libb_local_rel),
        other => panic!("expected Path (replace override), got {:?}", other),
    }

    fs::remove_dir_all(&libb).ok();
    fs::remove_dir_all(&libb_local).ok();
    fs::remove_dir_all(&consumer).ok();
    fs::remove_dir_all(&cache_home).ok();
}

/// `[replace]` referencing a name absent from `[dependencies]` — dedicated
/// diagnostic (`W_REPLACE_UNKNOWN_DEP`), nothing to replace.
#[test]
fn replace_unknown_dep_warns() {
    let dir = unique("replace_unknown_int");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("nova.toml"),
        "[package]\nname = \"app\"\n[lib]\nsrc = \".\"\n\
         [replace]\nghost = { path = \"../ghost\" }\n",
    )
    .unwrap();
    let m = manifest::parse_manifest(&dir.join("nova.toml"), &dir).expect("parse");
    let ws = manifest::manifest_warnings(&m, &dir.join("nova.toml"));
    assert_eq!(ws.len(), 1);
    assert_eq!(ws[0].code, "W_REPLACE_UNKNOWN_DEP");
    fs::remove_dir_all(&dir).ok();
}

/// Bare `path`-dep in `[dependencies]` (no release form at all, no
/// `[replace]` involved) — `W_DEP_PATH_NO_RELEASE`, matching the existing
/// Plan 202/203 `{ path = "../nova-tls" }` pattern (must keep compiling —
/// warning only).
#[test]
fn bare_path_dep_warns_but_resolves() {
    let leaf = unique("leaf_bare");
    fs::create_dir_all(&leaf).unwrap();
    fs::write(
        leaf.join("nova.toml"),
        "[package]\nname = \"leaf\"\n[lib]\nsrc = \".\"\n",
    )
    .unwrap();

    let app = unique("app_bare");
    fs::create_dir_all(&app).unwrap();
    let rel = pathdiff(&app, &leaf);
    fs::write(
        app.join("nova.toml"),
        format!(
            "[package]\nname = \"app\"\n[lib]\nsrc = \".\"\n\
             [dependencies]\nleaf = {{ path = \"{}\" }}\n",
            rel,
        ),
    )
    .unwrap();

    let m = manifest::parse_manifest(&app.join("nova.toml"), &app).expect("parse");
    let ws = manifest::manifest_warnings(&m, &app.join("nova.toml"));
    assert_eq!(ws.len(), 1);
    assert_eq!(ws[0].code, "W_DEP_PATH_NO_RELEASE");

    // Still resolves fine (warning, not error) — collect_dep_graph succeeds.
    let graph = lockfile::collect_dep_graph(&app).expect("graph resolves despite warning");
    assert_eq!(graph.len(), 1);
    assert_eq!(graph[0].name, "leaf");
    assert!(matches!(graph[0].source, LockedSource::Path { .. }));
    fs::remove_dir_all(&leaf).ok();
    fs::remove_dir_all(&app).ok();
}

/// Real-world smoke (owner instruction, Plan 204): resolve `nova-tls`
/// `v0.1.0` from the ACTUAL sibling repo via a `file://` URL (git supports
/// local-path transports uniformly — same codepath as a real GitHub clone,
/// fully offline/deterministic). Skips gracefully if the sibling repo or
/// tag isn't present (CI checkout without nova-tls sibling).
#[test]
fn resolve_real_nova_tls_v0_1_0_via_file_url() {
    // `cargo test`'s cwd is this crate's manifest dir (`<worktree>/compiler-codegen`);
    // nova-tls is a sibling of the WORKTREE root (`<parent-of-worktree>/nova-tls`),
    // so check both one and two levels up (plain worktree checkout vs.
    // running from the crate dir directly).
    let cwd = std::env::current_dir().unwrap();
    let nova_tls_dir = ["../nova-tls", "../../nova-tls", "../../../nova-tls"]
        .iter()
        .find_map(|rel| {
            let p = cwd.join(rel);
            match p.canonicalize() {
                Ok(p) if p.join(".git").exists() => Some(p),
                _ => None,
            }
        });
    let nova_tls_dir = match nova_tls_dir {
        Some(p) => p,
        None => {
            eprintln!("skip: sibling nova-tls repo not found next to worktree — not fatal");
            return;
        }
    };
    let tag_check = Command::new("git")
        .args(["-C", &nova_tls_dir.to_string_lossy(), "tag", "-l", "v0.1.0"])
        .output()
        .expect("run git tag -l");
    let has_tag = String::from_utf8_lossy(&tag_check.stdout).trim() == "v0.1.0";
    if !has_tag {
        eprintln!("skip: nova-tls has no v0.1.0 tag locally — not fatal");
        return;
    }

    // `canonicalize()` on Windows returns an extended-length `\\?\`-prefixed
    // path — strip it before building the `file://` URL (git's URL parser
    // doesn't understand the Windows verbatim prefix).
    let clean = nova_tls_dir
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .replace('\\', "/");
    let url = format!("file:///{}", clean);

    let consumer = unique("consumer_real_tls");
    fs::create_dir_all(&consumer).unwrap();
    fs::write(
        consumer.join("nova.toml"),
        format!(
            "[package]\nname = \"app\"\n[lib]\nsrc = \".\"\n\
             [dependencies]\ntls = {{ git = \"{}\", version = \"0.1\" }}\n",
            url,
        ),
    )
    .unwrap();

    let cache_home = unique("home_real_tls");
    std::env::set_var("NOVA_HOME", &cache_home);
    let res = lockfile::sync(&consumer);
    std::env::remove_var("NOVA_HOME");
    assert!(res.is_ok(), "sync real nova-tls via file://: {:?}", res.err());

    let lock = lockfile::load(&consumer).expect("load").expect("lock exists");
    assert_eq!(lock.packages.len(), 1);
    match &lock.packages[0].source {
        LockedSource::Git { version, commit, .. } => {
            assert_eq!(version.as_deref(), Some("0.1.0"), "^0.1 resolves to tag v0.1.0");
            assert_eq!(commit.len(), 40, "full commit hash recorded");
        }
        other => panic!("expected Git lock entry, got {:?}", other),
    }

    fs::remove_dir_all(&consumer).ok();
    fs::remove_dir_all(&cache_home).ok();
}

/// Naive relative-path from `from` to `to` (both must exist) — good enough
/// for temp-dir siblings under the same OS temp root (no `..`-nesting
/// beyond one level needed in these fixtures).
fn pathdiff(from: &Path, to: &Path) -> String {
    let from_parent = from.parent().unwrap();
    let to_name = to.file_name().unwrap().to_string_lossy();
    if from_parent == to.parent().unwrap() {
        format!("../{}", to_name)
    } else {
        to.to_string_lossy().replace('\\', "/")
    }
}
