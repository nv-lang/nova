// Registry 822 -- the compiler must name ITS OWN deficit, and only when the
// program actually ran into it.
//
// `nova check hello.nv` on the VERBATIM hello-world from docs/guide/quickstart.md,
// run where the standard library cannot be found, used to answer `undefined
// identifier `println``. `println` exists (std/src/prelude/runtime.nv); what was
// missing was the prelude, and about that there was not one word -- so the reader
// goes hunting for a mistake in a file that has none. The import resolver answers
// the SAME deficit loudly (`cannot find module ... searched: <paths>`): one
// deficit, two opposite reactions from one compiler.
//
// The trigger is not exotic: our own quickstart warns that without the leading dot
// in `. ./setup-env.ps1` the variables never get set.
//
// THREE CASES, AND THE THIRD IS WHY THIS FILE LOOKS LIKE THIS. Two earlier
// attempts at this fix were reverted, both because they turned the deficit into a
// REFUSAL upstream of the place it becomes an error:
//
//   * refusing inside `compute_prelude_imports` broke six unit tests that pass a
//     directory literally named `no_stdlib` and assert that import resolution
//     still succeeds -- a stated property of that layer;
//   * refusing at the `check_pipeline` boundary broke a guard selftest that
//     compiles a trivial temporary file with the real binary. That refusal fails
//     ANY compilation without a reachable prelude, including programs that do not
//     need one.
//
// So `needs_nothing_from_the_prelude_still_builds` is not a nicety. It is the
// case that caught the second attempt, and a fixture without it would have passed
// on a compiler that refuses everything.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn nova() -> Command {
    Command::new(env!("CARGO_BIN_EXE_nova"))
}

/// The repository's real std, reached from this crate rather than from the
/// caller's environment -- the test must not depend on the variable it is about.
fn repo_std() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("nova-cli has a parent")
        .join("std")
}

/// A directory OUTSIDE the repository, so walking up from it finds no `std/` and
/// no `nova.toml`. Without this the fallback `<project root>/std` finds the real
/// standard library and the probe answers a different question -- which is
/// exactly how the first attempt to reproduce this defect concluded, wrongly,
/// that it no longer existed.
fn isolated(tag: &str, body: &str) -> PathBuf {
    let mut dir = std::env::temp_dir();
    dir.push(format!("nova_prelude_822_{}_{}", std::process::id(), tag));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("mkdir temp dir");
    fs::write(dir.join("m.nv"), body).expect("write source");
    dir
}

const USES_PRELUDE: &str = "module m\n\nfn main() Io -> () {\n    println(\"hi\")\n}\n";
const USES_NOTHING: &str = "module m\n\nfn main() -> int => 0\n";

fn run_in(dir: &PathBuf, with_std: bool) -> (Option<i32>, String) {
    let mut c = nova();
    c.arg("check").arg("m.nv").current_dir(dir);
    if with_std {
        c.env("NOVA_STD_PATH", repo_std());
    } else {
        c.env_remove("NOVA_STD_PATH");
    }
    let out = c.output().expect("run nova check");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.code(), text)
}

#[test]
fn without_the_prelude_the_compiler_names_its_own_deficit() {
    let dir = isolated("missing", USES_PRELUDE);
    let (_code, all) = run_in(&dir, false);

    assert!(
        all.contains("cannot find the standard library prelude"),
        "the refusal must name the compiler's own missing half; got:\n{}",
        all
    );
    assert!(
        all.contains("searched:"),
        "it must list what it looked at, as the import resolver does; got:\n{}",
        all
    );

    // The deficit must come FIRST. A cause printed after its consequences is
    // read as one more consequence, which is the defect this row is about.
    let cause = all
        .find("cannot find the standard library prelude")
        .expect("cause present");
    if let Some(consequence) = all.find("undefined identifier") {
        assert!(
            cause < consequence,
            "the cause must precede the consequences; got:\n{}",
            all
        );
    }
}

#[test]
fn with_the_prelude_the_same_file_compiles() {
    let dir = isolated("present", USES_PRELUDE);
    let (code, all) = run_in(&dir, true);
    assert_eq!(code, Some(0), "the same file, with std reachable:\n{}", all);
}

#[test]
fn needs_nothing_from_the_prelude_still_builds() {
    // The case that reverted attempt two. A program referencing no prelude name
    // must compile with no standard library at all: nothing is missing FOR IT.
    let dir = isolated("nothing", USES_NOTHING);
    let (code, all) = run_in(&dir, false);
    assert_eq!(
        code,
        Some(0),
        "a program needing no prelude must not be refused for its absence:\n{}",
        all
    );
    assert!(
        !all.contains("cannot find the standard library prelude"),
        "and it must not even be told about it:\n{}",
        all
    );
}
