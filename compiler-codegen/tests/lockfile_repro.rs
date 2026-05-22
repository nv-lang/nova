//! Plan 03.1 Ф.4 — `nova.lock` фиксирует git-зависимость: после
//! `sync` ветка `branch`-зависимости «не уезжает» даже если в upstream
//! появились новые коммиты.
//!
//! Один `#[test]` в файле — `NOVA_HOME` ставится глобально без гонки.

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
        "nova_lockrepro_{}_{}_{}",
        tag,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

#[test]
fn lockfile_pins_git_branch_against_drift() {
    // --- git-репозиторий-источник: пакет `gitlib`, ветка по умолчанию ---
    let src = unique("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(
        src.join("nova.toml"),
        "[package]\nname = \"gitlib\"\nversion = \"0.1.0\"\n[lib]\nsrc = \".\"\n",
    )
    .unwrap();
    fs::write(src.join("calc.nv"), "module gitlib.calc\n\nexport fn v() -> int => 1\n").unwrap();
    git(&["init", "--quiet", &src.to_string_lossy()], None);
    git(&["config", "user.email", "t@t"], Some(&src));
    git(&["config", "user.name", "t"], Some(&src));
    git(&["add", "-A"], Some(&src));
    git(&["commit", "--quiet", "-m", "c1"], Some(&src));
    let branch = git(&["branch", "--show-current"], Some(&src));
    let c1 = git(&["rev-parse", "HEAD"], Some(&src));

    // --- проект-потребитель: git-зависимость по ветке -------------------
    let consumer = unique("consumer");
    fs::create_dir_all(&consumer).unwrap();
    fs::write(
        consumer.join("nova.toml"),
        format!(
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n[lib]\nsrc = \".\"\n\
             [dependencies]\ngitlib = {{ git = \"{}\", branch = \"{}\" }}\n",
            src.to_string_lossy().replace('\\', "/"),
            branch,
        ),
    )
    .unwrap();

    let cache_home = unique("home");
    std::env::set_var("NOVA_HOME", &cache_home);

    // --- 1-й sync: фиксирует текущий commit ветки (c1) ------------------
    lockfile::sync(&consumer).expect("first sync");
    let lock1 = lockfile::load(&consumer).expect("load lock").expect("lock exists");
    let locked1 = lock1
        .packages
        .iter()
        .find(|p| p.name == "gitlib")
        .map(|p| match &p.source {
            LockedSource::Git { commit, .. } => commit.clone(),
            _ => panic!("ожидалась git-запись"),
        })
        .expect("gitlib в lock");
    assert_eq!(locked1, c1, "lock должен зафиксировать c1");

    // --- upstream двигается: новый commit c2 на той же ветке ------------
    fs::write(src.join("calc.nv"), "module gitlib.calc\n\nexport fn v() -> int => 2\n").unwrap();
    git(&["add", "-A"], Some(&src));
    git(&["commit", "--quiet", "-m", "c2"], Some(&src));
    let c2 = git(&["rev-parse", "HEAD"], Some(&src));
    assert_ne!(c1, c2);

    // --- 2-й sync: lock существует → ветка зафиксирована на c1 ----------
    lockfile::sync(&consumer).expect("second sync");
    let lock2 = lockfile::load(&consumer).expect("load lock 2").expect("lock 2");
    let locked2 = lock2
        .packages
        .iter()
        .find(|p| p.name == "gitlib")
        .map(|p| match &p.source {
            LockedSource::Git { commit, .. } => commit.clone(),
            _ => panic!("git-запись"),
        })
        .expect("gitlib в lock 2");
    assert_eq!(locked2, c1, "после 2-го sync ветка не должна уехать на c2");

    std::env::remove_var("NOVA_HOME");
    fs::remove_dir_all(&src).ok();
    fs::remove_dir_all(&consumer).ok();
    fs::remove_dir_all(&cache_home).ok();
}
