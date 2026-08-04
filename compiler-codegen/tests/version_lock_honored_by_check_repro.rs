//! №336 regression: the ACTUAL defect was not `lockfile::sync` (which
//! already honored the lock correctly — see `version_lock_repro.rs` and
//! `version_lock_transitive_via_path_repro.rs`), but that `nova check` /
//! `nova test` never call `sync`/`load_pins` anywhere on their path —
//! they go straight to `imports::resolve_imports_inline`, which for a
//! git dependency pinned by a semver RANGE used to always live-resolve to
//! the newest matching tag, silently ignoring a perfectly valid
//! `nova.lock.toml` sitting right next to the manifest.
//!
//! This test exercises `imports::resolve_imports_inline` DIRECTLY (the
//! exact call `nova-cli`'s `check_one_file` makes) — NOT `lockfile::sync` —
//! against a git+version dependency with a PRE-EXISTING lock pinning the
//! OLDER tag, while a newer tag exists upstream. Before the fix
//! (`lockfile::ensure_pins_loaded` wired into `imports::lookup_dependency`
//! / `resolved_dependency_roots`), this pulled in `v2` unconditionally;
//! after the fix it must honor the lock and pull in `v1`.

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
        "nova_vlhc_{}_{}_{}",
        tag,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

#[test]
fn nova_check_path_honors_lock_against_newer_upstream_tag() {
    // --- lib: git-пакет, v1.0.0 экспортирует `v1_marker`, v1.5.0 —
    //     `v2_marker` (разные имена — легко отличить, что реально смёржилось).
    //     ОБЕ версии подпадают под диапазон `^1.0` зависимости консьюмера —
    //     иначе тест не отличал бы «держит лок» от «просто уважает диапазон»
    //     (v2.0.0 был бы отфильтрован диапазоном И БЕЗ фикса — ложный PASS,
    //     ровно так тест-баг был пойман в ходе разведки этого окна).
    let lib = unique("lib");
    fs::create_dir_all(&lib).unwrap();
    let ld = lib.to_string_lossy().to_string();
    fs::write(
        lib.join("nova.toml"),
        "[package]\nname = \"lib\"\nversion = \"1.0.0\"\n[lib]\nsrc = \".\"\n",
    )
    .unwrap();
    fs::write(
        lib.join("core.nv"),
        "module lib.core\n\nexport fn v1_marker() -> int => 1\n",
    )
    .unwrap();
    git(&["init", "--quiet", &ld], None);
    git(&["-C", &ld, "config", "user.email", "t@t"], None);
    git(&["-C", &ld, "config", "user.name", "t"], None);
    git(&["-C", &ld, "add", "-A"], None);
    git(&["-C", &ld, "commit", "--quiet", "-m", "v1"], None);
    git(&["-C", &ld, "tag", "v1.0.0"], None);
    let v1_commit = git(&["-C", &ld, "rev-parse", "HEAD"], None);

    fs::write(
        lib.join("core.nv"),
        "module lib.core\n\nexport fn v2_marker() -> int => 2\n",
    )
    .unwrap();
    git(&["-C", &ld, "add", "-A"], None);
    git(&["-C", &ld, "commit", "--quiet", "-m", "v1.5"], None);
    git(&["-C", &ld, "tag", "v1.5.0"], None);
    let lib_url = lib.to_string_lossy().replace('\\', "/");

    // --- consumer: lib ^1.0, с lock, УЖЕ зафиксированным на v1.0.0 ------
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
    fs::write(
        consumer.join("nova.lock.toml"),
        format!(
            "version = 1\n\n[[package]]\n\
             name = \"lib\"\n\
             source = \"git\"\n\
             git = \"{}\"\n\
             pin = \"version:^1.0\"\n\
             version = \"1.0.0\"\n\
             commit = \"{}\"\n",
            lib_url, v1_commit,
        ),
    )
    .unwrap();
    fs::write(
        consumer.join("main.nv"),
        "module app\n\nimport lib.core.{v1_marker}\n\nexport fn main() -> int => 0\n",
    )
    .unwrap();

    let cache_home = unique("home");
    std::env::set_var("NOVA_HOME", &cache_home);

    // Замер: `imports::resolve_imports_inline` — ТОТ ЖЕ вызов, который
    // делает `check_one_file` (nova-cli) — БЕЗ какого-либо `lockfile::sync`
    // где-либо на этом пути.
    let entry = consumer.join("main.nv");
    let src = fs::read_to_string(&entry).unwrap();
    let mut module = nova_codegen::parser::parse(&src).expect("parse entry");
    let stdlib_dir = consumer.join("__no_std__"); // не существует — ок, prelude no-op.
    let res = nova_codegen::imports::resolve_imports_inline(
        &entry, &mut module, &consumer, &stdlib_dir,
    );

    std::env::remove_var("NOVA_HOME");

    res.expect("resolve_imports_inline must succeed honoring the lock");

    let names: Vec<String> = module
        .items
        .iter()
        .filter_map(|it| match it {
            nova_codegen::ast::Item::Fn(f) => Some(f.name.clone()),
            _ => None,
        })
        .collect();

    fs::remove_dir_all(&lib).ok();
    fs::remove_dir_all(&consumer).ok();
    fs::remove_dir_all(&cache_home).ok();

    assert!(
        names.iter().any(|n| n == "v1_marker"),
        "ожидался v1_marker (лок держит v1.0.0) — items: {:?}",
        names,
    );
    assert!(
        !names.iter().any(|n| n == "v2_marker"),
        "v2_marker означает, что резолвер проигнорировал лок и взял МАКСИМАЛЬНЫЙ \
         тег (v2.0.0) — items: {:?}",
        names,
    );
}
