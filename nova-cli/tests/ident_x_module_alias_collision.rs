//! [M-202-ident-x-module-alias-collision] regression pin.
//!
//! `nova build`/`nova test` (NOT `nova check`) on a program importing ANY
//! module whose last path segment is `x` (file `x.nv`) used to fail with a
//! false `[E7401] no function 'compare' in module 'x'` — completely
//! unrelated to the user's own import.
//!
//! Root cause (found during investigation — the original marker's own
//! `auto_derive.rs` hypothesis did NOT pan out): `std/src/collections/
//! vec_iter/core.nv` (and the mirror `vec_lazy/core.nv`) declared the
//! generic `@min()`/`@max()` iterator methods with a match-arm bind named
//! bare `x` (`Some(x) => { if x.compare(best) < 0 { best = x } }`).
//! `types/mod.rs`'s `f1_expr` (`ExprKind::Match` arm, checks
//! `match_arm_bindings`) could not resolve the GENERIC `Option[T]`
//! scrutinee type of `@next()` while checking these generic method bodies
//! themselves (T is still abstract), so `x` was never inserted into
//! `scope`. `f1_check_call`'s `ExprKind::Member` dispatch (D289 module-call
//! path) then fell through its `scope.contains_key(prefix)` guard straight
//! to the `imported_modules.contains(prefix)` branch — which is a
//! COMPILE-UNIT-WIDE set, so ANY user import ending in `.x` (e.g. `import
//! a.neg.x.{who}`) made `"x"` a known module alias, and `x.compare(best)`
//! got misread as a call to a (non-existent) free function `compare` in
//! module `x`.
//!
//! Fix: renamed the colliding std-library bind from `x` to `cand` in both
//! files (hygiene at the actual collision site — cheaper and lower-risk
//! than teaching the checker to distinguish an unresolved-generic local
//! from a module alias). `auto_derive.rs` and `types/mod.rs` are untouched.
//!
//! This test builds AND RUNS a real cross-file two-module program that
//! reproduces the marker's exact repro shape (`import a.neg.x.{who}`) —
//! `spec_tests/conformance` is a single folder-module CU and can't express
//! a genuinely separate imported package/module boundary, hence a CLI-level
//! integration test (precedent: `lint_deny.rs`, `plan204_local_toml_and_
//! replace_gate.rs`).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Repo root — `nova-cli`'s parent (mirrors `entry_folder_module.rs`).
/// `nova build` resolves its OWN toolchain/std/runtime paths from the
/// process's CWD (`find_repo_root()`), so pointing CWD at the real repo
/// gives a working std/GC/libuv setup for free — the fixture PACKAGE
/// itself (import resolution for `a.neg.x`) is resolved independently,
/// walking UP from the entry FILE's own path to ITS OWN `nova.toml`.
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("nova-cli has a parent dir")
        .to_path_buf()
}

fn unique(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "nova_identx_{}_{}_{}",
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

fn combined_output(out: &std::process::Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// Isolated two-module package: `src/a/neg/x.nv` (`module neg.x`, exports
/// `who`) + `src/main.nv` (`module identx_repro`, `import a.neg.x.{who}`).
/// Mirrors the marker's own repro (`import a.neg.x.{who}`, D78 rev-3 path
/// research note `docs/dev/research/2026-07-13-module-naming-two-segment-
/// review.md` §2а — the same `a/neg/x.nv` shape).
fn write_fixture(dir: &Path) {
    write_file(
        &dir.join("nova.toml"),
        "[package]\nname = \"identx_repro\"\nversion = \"0.1.0\"\nnova-version = \"0.5\"\n\
         [lib]\nsrc = \"src\"\n\n[dependencies]\n",
    );
    write_file(
        &dir.join("src").join("a").join("neg").join("x.nv"),
        "module neg.x\n\nexport fn who() -> str => \"x-module\"\n",
    );
    write_file(
        &dir.join("src").join("main.nv"),
        "module identx_repro\n\nimport a.neg.x.{who}\n\nfn main() {\n    println(who())\n}\n",
    );
}

#[test]
fn nova_build_does_not_false_positive_e7401_on_module_named_x() {
    let repo = repo_root();
    let dir = unique("build");
    write_fixture(&dir);

    let main_nv = dir.join("src").join("main.nv");
    let out_bin = dir.join(if cfg!(windows) { "app.exe" } else { "app" });

    let out = Command::new(env!("CARGO_BIN_EXE_nova"))
        .arg("build")
        .arg(&main_nv)
        .arg("-o")
        .arg(&out_bin)
        .current_dir(&repo)
        .output()
        .expect("failed to spawn `nova build`");
    let combined = combined_output(&out);

    assert!(
        !combined.contains("E7401"),
        "[M-202-ident-x-module-alias-collision] regressed: false E7401 on a \
         program merely importing a module whose last path segment is `x`.\n{}",
        combined
    );
    assert!(
        out.status.success(),
        "`nova build` on the `a.neg.x` fixture must succeed (was failing with \
         E7401 before the fix); status={:?}\n{}",
        out.status,
        combined
    );

    // Run the produced binary — confirms the fix didn't just silence the
    // diagnostic but that the program is genuinely correct end to end.
    let run_out = Command::new(&out_bin).output();
    fs::remove_dir_all(&dir).ok();
    match run_out {
        Ok(run_out) => {
            let run_combined = combined_output(&run_out);
            assert!(
                run_out.status.success() && run_combined.contains("x-module"),
                "built binary did not run/print as expected: {}",
                run_combined
            );
        }
        Err(e) => panic!("failed to run built binary {}: {}", out_bin.display(), e),
    }
}

/// Same fixture, but with the colliding module's LAST segment renamed away
/// from `x` (`helper` — the marker's own noted workaround) — sanity control:
/// this must ALWAYS have passed (before and after the fix), so a failure
/// here would mean the fixture itself is broken, not that the `x`-specific
/// bug reappeared.
#[test]
fn nova_build_control_non_x_module_name_always_works() {
    let repo = repo_root();
    let dir = unique("control");
    write_file(
        &dir.join("nova.toml"),
        "[package]\nname = \"identx_repro_ctl\"\nversion = \"0.1.0\"\nnova-version = \"0.5\"\n\
         [lib]\nsrc = \"src\"\n\n[dependencies]\n",
    );
    write_file(
        &dir.join("src").join("a").join("neg").join("helper.nv"),
        "module neg.helper\n\nexport fn who() -> str => \"helper-module\"\n",
    );
    write_file(
        &dir.join("src").join("main.nv"),
        "module identx_repro_ctl\n\nimport a.neg.helper.{who}\n\nfn main() {\n    println(who())\n}\n",
    );

    let main_nv = dir.join("src").join("main.nv");
    let out_bin = dir.join(if cfg!(windows) { "app.exe" } else { "app" });
    let out = Command::new(env!("CARGO_BIN_EXE_nova"))
        .arg("build")
        .arg(&main_nv)
        .arg("-o")
        .arg(&out_bin)
        .current_dir(&repo)
        .output()
        .expect("failed to spawn `nova build`");
    let combined = combined_output(&out);
    fs::remove_dir_all(&dir).ok();

    assert!(
        out.status.success(),
        "control fixture (module `neg.helper`, not `neg.x`) must build; status={:?}\n{}",
        out.status,
        combined
    );
}
