// Plan 156 / D274: the tree-walking interpreter is currently UNSUPPORTED.
//
// `nova run` must error out with a clear message (NEGATIVE), while the normal
// C-codegen front-end path (`nova check`) must keep working (POSITIVE). Both
// assertions are exercised against the actually-built `nova` binary, so they
// are an end-to-end guard on the CLI contract, not a mock.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn nova() -> Command {
    Command::new(env!("CARGO_BIN_EXE_nova"))
}

/// Create an ISOLATED temp directory holding a single trivial `.nv` file and
/// return `(dir, file)`. Isolation is required: Nova treats a directory as one
/// folder-module of co-equal files, so writing into the shared system temp dir
/// (alongside other `.nv` files) would fold them into one module and collide on
/// `fn main`. Each test gets its own directory and removes it afterwards.
fn isolated_nv(tag: &str) -> (PathBuf, PathBuf) {
    let mut dir = std::env::temp_dir();
    dir.push(format!("nova_interp_unsupported_{}_{}", std::process::id(), tag));
    fs::create_dir_all(&dir).expect("mkdir temp dir");
    let file = dir.join("main.nv");
    fs::write(&file, "module t\n\nfn main() {\n}\n").expect("write temp .nv");
    (dir, file)
}

/// NEGATIVE: `nova run FILE` must NOT run an interpreter — it must exit nonzero
/// and tell the user the interpreter is currently unsupported, pointing at the
/// C-codegen path.
#[test]
fn nova_run_is_unsupported() {
    let (dir, file) = isolated_nv("run");
    let out = nova().arg("run").arg(&file).output().expect("spawn `nova run`");
    let _ = fs::remove_dir_all(&dir);

    assert!(
        !out.status.success(),
        "`nova run` must exit nonzero while the interpreter is unsupported"
    );
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
    .to_lowercase();
    assert!(
        combined.contains("interpreter") && combined.contains("not supported"),
        "`nova run` must explain the interpreter is not supported; got:\n{combined}"
    );
    assert!(
        combined.contains("nova build") || combined.contains("nova test"),
        "`nova run` error must point to `nova build` / `nova test`; got:\n{combined}"
    );
}

/// POSITIVE: the supported C-codegen front-end (`nova check`) still type-checks
/// a trivial program and exits 0 — disabling the interpreter did not break the
/// real toolchain.
#[test]
fn nova_check_still_works() {
    let (dir, file) = isolated_nv("check");
    let out = nova().arg("check").arg(&file).output().expect("spawn `nova check`");
    let _ = fs::remove_dir_all(&dir);

    assert!(
        out.status.success(),
        "`nova check` must still succeed; stdout=\n{}\nstderr=\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}
