//! Plan 03.2 Ф.4 — `nova.lock` фиксирует выбранную версию git-зависимости;
//! повторный `sync` её НЕ двигает (воспроизводимость), даже если в
//! upstream появился более новый тег. `nova update` — пере-резолвит.
//!
//! Один `#[test]` в файле — `NOVA_HOME` ставится глобально без гонки.

use nova_codegen::lockfile::{self, LockedSource};
use nova_codegen::semver::Version;
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
        "nova_vlr_{}_{}_{}",
        tag,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

/// Версия git-записи `name` в `nova.lock` пакета `pkg`.
fn locked_version(pkg: &Path, name: &str) -> Option<String> {
    let lock = lockfile::load(pkg).expect("load").expect("lock exists");
    lock.packages.iter().find(|p| p.name == name).and_then(|p| match &p.source {
        LockedSource::Git { version, .. } => version.clone(),
        _ => None,
    })
}

#[test]
fn lock_pins_version_against_upstream_tags() {
    // --- git-пакет `lib` с тегом v1.0.0 -------------------------------
    let lib = unique("lib");
    fs::create_dir_all(&lib).unwrap();
    let ld = lib.to_string_lossy().to_string();
    fs::write(
        lib.join("nova.toml"),
        "[package]\nname = \"lib\"\nversion = \"1.0.0\"\n[lib]\nsrc = \".\"\n",
    )
    .unwrap();
    fs::write(lib.join("core.nv"), "module lib.core\n\nexport fn v() -> int => 1\n").unwrap();
    git(&["init", "--quiet", &ld], None);
    git(&["-C", &ld, "config", "user.email", "t@t"], None);
    git(&["-C", &ld, "config", "user.name", "t"], None);
    git(&["-C", &ld, "add", "-A"], None);
    git(&["-C", &ld, "commit", "--quiet", "-m", "v1"], None);
    git(&["-C", &ld, "tag", "v1.0.0"], None);
    let lib_url = lib.to_string_lossy().replace('\\', "/");

    // --- проект-потребитель: lib ^1.0 ---------------------------------
    let consumer = unique("consumer");
    fs::create_dir_all(&consumer).unwrap();
    fs::write(
        consumer.join("nova.toml"),
        format!(
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n[lib]\nsrc = \".\"\n\
             [dependencies]\nlib = {{ git = \"{}\", version = \"^1.0\" }}\n",
            lib_url,
        ),
    )
    .unwrap();

    let cache_home = unique("home");
    std::env::set_var("NOVA_HOME", &cache_home);

    // --- 1-й sync: фиксирует v1.0.0 -----------------------------------
    lockfile::sync(&consumer).expect("first sync");
    assert_eq!(
        locked_version(&consumer, "lib").as_deref(),
        Some("1.0.0"),
        "1-й sync фиксирует 1.0.0",
    );

    // --- upstream: новый тег v1.5.0 -----------------------------------
    fs::write(lib.join("core.nv"), "module lib.core\n\nexport fn v() -> int => 5\n").unwrap();
    git(&["-C", &ld, "add", "-A"], None);
    git(&["-C", &ld, "commit", "--quiet", "-m", "v1.5"], None);
    git(&["-C", &ld, "tag", "v1.5.0"], None);

    // --- 2-й sync: lock держит 1.0.0 (НЕ прыгает на 1.5.0) ------------
    lockfile::sync(&consumer).expect("second sync");
    assert_eq!(
        locked_version(&consumer, "lib").as_deref(),
        Some("1.0.0"),
        "2-й sync не должен сдвинуть версию (воспроизводимость)",
    );

    // --- nova update: пере-резолв → 1.5.0 -----------------------------
    lockfile::update(&consumer, None).expect("update");
    assert_eq!(
        locked_version(&consumer, "lib").as_deref(),
        Some("1.5.0"),
        "update пере-резолвит в новейшую в пределах ^1.0",
    );

    // --- nova update --precise lib@1.0.0: точная фиксация -------------
    lockfile::update_precise(&consumer, "lib", &lib_url, &Version::parse("1.0.0").unwrap())
        .expect("update --precise");
    assert_eq!(
        locked_version(&consumer, "lib").as_deref(),
        Some("1.0.0"),
        "--precise фиксирует точную версию (откат с 1.5.0 на 1.0.0)",
    );

    std::env::remove_var("NOVA_HOME");
    fs::remove_dir_all(&lib).ok();
    fs::remove_dir_all(&consumer).ok();
    fs::remove_dir_all(&cache_home).ok();
}
