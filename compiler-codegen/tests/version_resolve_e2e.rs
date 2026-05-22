//! Plan 03.2 Ф.3 — end-to-end: проект с версионными git-зависимостями
//! резолвится через backtracking-резолвер; `nova.lock` фиксирует
//! согласованные версии всего (транзитивного) дерева.
//!
//! Источники — локальные `git init`-репозитории с semver-тегами:
//! тест offline и детерминирован. Один `#[test]` — `NOVA_HOME`
//! ставится глобально без гонки.

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
        "nova_vre_{}_{}_{}",
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

#[test]
fn version_deps_resolve_transitively() {
    // --- libb: пакет с тегами v1.0.0 и v2.0.0 --------------------------
    let libb = unique("libb");
    init_repo(&libb);
    fs::write(
        libb.join("nova.toml"),
        "[package]\nname = \"libb\"\nversion = \"1.0.0\"\n[lib]\nsrc = \".\"\n",
    )
    .unwrap();
    fs::write(libb.join("core.nv"), "module libb.core\n\nexport fn b() -> int => 1\n").unwrap();
    let libb_v1 = commit_tag(&libb, "v1.0.0");
    fs::write(libb.join("core.nv"), "module libb.core\n\nexport fn b() -> int => 2\n").unwrap();
    let _libb_v2 = commit_tag(&libb, "v2.0.0");
    let libb_url = libb.to_string_lossy().replace('\\', "/");

    // --- liba: v1.0.0 без deps; v1.1.0 зависит от libb ^1.0 -----------
    let liba = unique("liba");
    init_repo(&liba);
    fs::write(
        liba.join("nova.toml"),
        "[package]\nname = \"liba\"\nversion = \"1.0.0\"\n[lib]\nsrc = \".\"\n",
    )
    .unwrap();
    fs::write(liba.join("api.nv"), "module liba.api\n\nexport fn a() -> int => 10\n").unwrap();
    let _liba_v1 = commit_tag(&liba, "v1.0.0");
    fs::write(
        liba.join("nova.toml"),
        format!(
            "[package]\nname = \"liba\"\nversion = \"1.1.0\"\n[lib]\nsrc = \".\"\n\
             [dependencies]\nlibb = {{ git = \"{}\", version = \"^1.0\" }}\n",
            libb_url,
        ),
    )
    .unwrap();
    let liba_v11 = commit_tag(&liba, "v1.1.0");
    let liba_url = liba.to_string_lossy().replace('\\', "/");

    // --- проект-потребитель: liba ^1.0 ---------------------------------
    let consumer = unique("consumer");
    fs::create_dir_all(&consumer).unwrap();
    fs::write(
        consumer.join("nova.toml"),
        format!(
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n[lib]\nsrc = \".\"\n\
             [dependencies]\nliba = {{ git = \"{}\", version = \"^1.0\" }}\n",
            liba_url,
        ),
    )
    .unwrap();

    let cache_home = unique("home");
    std::env::set_var("NOVA_HOME", &cache_home);

    // --- sync: резолвер выбирает версии, nova.lock их фиксирует --------
    let res = lockfile::sync(&consumer);
    std::env::remove_var("NOVA_HOME");
    assert!(res.is_ok(), "sync version deps: {:?}", res.err());

    let lock = lockfile::load(&consumer)
        .expect("load lock")
        .expect("nova.lock создан");
    let commit_of = |name: &str| -> String {
        lock.packages
            .iter()
            .find(|p| p.name == name)
            .map(|p| match &p.source {
                LockedSource::Git { commit, .. } => commit.clone(),
                _ => panic!("`{}` — ожидалась git-запись", name),
            })
            .unwrap_or_else(|| panic!("`{}` отсутствует в nova.lock", name))
    };

    // liba ^1.0 → наибольшая 1.x → v1.1.0.
    assert_eq!(commit_of("liba"), liba_v11, "liba → v1.1.0");
    // liba@v1.1.0 требует libb ^1.0 → v1.0.0 (v2.0.0 вне диапазона).
    assert_eq!(commit_of("libb"), libb_v1, "libb → v1.0.0 (не v2.0.0)");

    fs::remove_dir_all(&libb).ok();
    fs::remove_dir_all(&liba).ok();
    fs::remove_dir_all(&consumer).ok();
    fs::remove_dir_all(&cache_home).ok();
}
