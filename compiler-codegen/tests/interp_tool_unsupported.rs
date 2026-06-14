// Q-interpreter-future / D274: the tree-walking interpreter is currently
// UNSUPPORTED. Plan 157 stubbed the user-facing `nova run`; this guard covers
// the INTERNAL dev binary `nova-codegen`, which still exposes two interpreter
// entry points (`run` and `test-interp`). Both must error out loudly, while the
// supported C-codegen path (`compile`) keeps working.
//
// The assertions spawn the actually-built `nova-codegen` binary (via
// `CARGO_BIN_EXE_nova-codegen`, injected by Cargo for integration tests of this
// crate), so they are an end-to-end guard on the dev-CLI contract, not a mock.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn nova_codegen() -> Command {
    Command::new(env!("CARGO_BIN_EXE_nova-codegen"))
}

/// Create an ISOLATED temp directory holding a single trivial `.nv` file and
/// return `(dir, file)`. Isolation is required: Nova treats a directory as one
/// folder-module of co-equal files, so writing into the shared system temp dir
/// (alongside other `.nv` files) would fold them into one module and collide on
/// `fn main`. Each test gets its own directory and removes it afterwards.
fn isolated_nv(tag: &str) -> (PathBuf, PathBuf) {
    let mut dir = std::env::temp_dir();
    dir.push(format!(
        "nova_codegen_interp_unsupported_{}_{}",
        std::process::id(),
        tag
    ));
    fs::create_dir_all(&dir).expect("mkdir temp dir");
    let file = dir.join("main.nv");
    fs::write(&file, "module t\n\nfn main() {\n}\n").expect("write temp .nv");
    (dir, file)
}

fn combined_lower(out: &std::process::Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
    .to_lowercase()
}

/// NEGATIVE: `nova-codegen run FILE` must NOT start an interpreter — it must
/// exit nonzero and tell the user the interpreter is currently not supported.
#[test]
fn nova_codegen_run_is_unsupported() {
    let (dir, file) = isolated_nv("run");
    let out = nova_codegen()
        .arg("run")
        .arg(&file)
        .output()
        .expect("spawn `nova-codegen run`");
    let _ = fs::remove_dir_all(&dir);

    assert!(
        !out.status.success(),
        "`nova-codegen run` must exit nonzero while the interpreter is unsupported"
    );
    let combined = combined_lower(&out);
    assert!(
        combined.contains("interpreter") && combined.contains("not supported"),
        "`nova-codegen run` must explain the interpreter is not supported; got:\n{combined}"
    );
}

/// NEGATIVE: `nova-codegen test-interp FILE` must likewise refuse to run the
/// interpreter-driven test path — exit nonzero with the same explanation.
#[test]
fn nova_codegen_test_interp_is_unsupported() {
    let (dir, file) = isolated_nv("test_interp");
    let out = nova_codegen()
        .arg("test-interp")
        .arg(&file)
        .output()
        .expect("spawn `nova-codegen test-interp`");
    let _ = fs::remove_dir_all(&dir);

    assert!(
        !out.status.success(),
        "`nova-codegen test-interp` must exit nonzero while the interpreter is unsupported"
    );
    let combined = combined_lower(&out);
    assert!(
        combined.contains("interpreter") && combined.contains("not supported"),
        "`nova-codegen test-interp` must explain the interpreter is not supported; got:\n{combined}"
    );
}

/// POSITIVE: the supported C-codegen path (`nova-codegen compile`) still
/// compiles a trivial program to C and exits 0 — disabling the interpreter did
/// not break the real toolchain.
#[test]
fn nova_codegen_compile_still_works() {
    let (dir, file) = isolated_nv("compile");
    let out = nova_codegen()
        .arg("compile")
        .arg(&file)
        .output()
        .expect("spawn `nova-codegen compile`");
    let _ = fs::remove_dir_all(&dir);

    assert!(
        out.status.success(),
        "`nova-codegen compile` must still succeed; stdout=\n{}\nstderr=\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}
