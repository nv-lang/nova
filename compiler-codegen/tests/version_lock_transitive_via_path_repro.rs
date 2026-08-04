//! Scratch repro (p336 investigation) — is a git+version dependency reached
//! ONLY transitively through a `path`-dependency (never the entry package's
//! own `[dependencies]`) still held by the lock across repeat `sync`, the
//! same way a DIRECT git+version dep is (per `version_lock_repro.rs`)?
//!
//! This mirrors `examples/nova.toml`'s real topology: `examples` depends on
//! `http` via `path`, and (transitively, through `polaris`/`http`'s own
//! manifest) on `compress`/`tls` via `git+version` — NOT the entry's own
//! `[dependencies]`.

use nova_codegen::lockfile::{self, LockedSource};
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
        "nova_vltp_{}_{}_{}",
        tag,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

fn commit_of(pkg: &Path, name: &str) -> String {
    let lock = lockfile::load(pkg).expect("load").expect("lock exists");
    lock.packages
        .iter()
        .find(|p| p.name == name && matches!(p.source, LockedSource::Git { .. }))
        .map(|p| match &p.source {
            LockedSource::Git { commit, .. } => commit.clone(),
            _ => unreachable!(),
        })
        .unwrap_or_else(|| panic!("`{}` git-запись отсутствует в lock", name))
}

#[test]
fn transitive_version_dep_via_path_is_held_across_sync() {
    // --- deepdep: git-пакет с тегом v1.0.0 -----------------------------
    let deepdep = unique("deepdep");
    fs::create_dir_all(&deepdep).unwrap();
    let dd = deepdep.to_string_lossy().to_string();
    fs::write(
        deepdep.join("nova.toml"),
        "[package]\nname = \"deepdep\"\nversion = \"1.0.0\"\n[lib]\nsrc = \".\"\n",
    )
    .unwrap();
    fs::write(deepdep.join("core.nv"), "module deepdep.core\n\nexport fn v() -> int => 1\n").unwrap();
    git(&["init", "--quiet", &dd], None);
    git(&["-C", &dd, "config", "user.email", "t@t"], None);
    git(&["-C", &dd, "config", "user.name", "t"], None);
    git(&["-C", &dd, "add", "-A"], None);
    git(&["-C", &dd, "commit", "--quiet", "-m", "v1"], None);
    git(&["-C", &dd, "tag", "v1.0.0"], None);
    let deepdep_url = deepdep.to_string_lossy().replace('\\', "/");

    // --- midpkg: PATH-пакет с деревом, объявляет deepdep git+version --
    let midpkg = unique("midpkg");
    fs::create_dir_all(&midpkg).unwrap();
    fs::write(
        midpkg.join("nova.toml"),
        format!(
            "[package]\nname = \"midpkg\"\nversion = \"1.0.0\"\n[lib]\nsrc = \".\"\n\
             [dependencies]\ndeepdep = {{ git = \"{}\", version = \"^1.0\" }}\n",
            deepdep_url,
        ),
    )
    .unwrap();
    fs::write(midpkg.join("mid.nv"), "module midpkg.mid\n\nexport fn m() -> int => 1\n").unwrap();

    // --- entry: PATH-дep на midpkg (НЕ git+version сам по себе) -------
    let entry = unique("entry");
    fs::create_dir_all(&entry).unwrap();
    fs::write(
        entry.join("nova.toml"),
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\n[lib]\nsrc = \".\"\n\
         [dependencies]\nmidpkg = { path = \"../midpkg\" }\n"
            .replace("../midpkg", &midpkg.to_string_lossy()),
    )
    .unwrap();

    let cache_home = unique("home");
    std::env::set_var("NOVA_HOME", &cache_home);

    // --- 1-й sync: фиксирует v1.0.0 (единственный тег) -----------------
    lockfile::sync(&entry).expect("first sync");
    let first_commit = commit_of(&entry, "deepdep");

    // --- upstream: новый тег v1.5.0 ------------------------------------
    fs::write(deepdep.join("core.nv"), "module deepdep.core\n\nexport fn v() -> int => 5\n").unwrap();
    git(&["-C", &dd, "add", "-A"], None);
    git(&["-C", &dd, "commit", "--quiet", "-m", "v1.5"], None);
    git(&["-C", &dd, "tag", "v1.5.0"], None);

    // --- 2-й sync: должен ДЕРЖАТЬ v1.0.0 (воспроизводимость) -----------
    lockfile::sync(&entry).expect("second sync");
    let second_commit = commit_of(&entry, "deepdep");

    std::env::remove_var("NOVA_HOME");
    fs::remove_dir_all(&deepdep).ok();
    fs::remove_dir_all(&midpkg).ok();
    fs::remove_dir_all(&entry).ok();
    fs::remove_dir_all(&cache_home).ok();

    assert_eq!(
        first_commit, second_commit,
        "транзитивная git+version зависимость (достижимая только через path-dep) \
         ДОЛЖНА держать зафиксированный commit между sync'ами, как и прямая — \
         first={} second={}",
        first_commit, second_commit,
    );
}
