// Registry 822 -- the compiler must say something about ITSELF, not blame the
// user's correct source.
//
// `nova check hello.nv` on the VERBATIM hello-world from docs/guide/quickstart.md
// used to answer `undefined identifier `println`` when the standard library was
// not reachable. `println` exists (std/src/prelude/runtime.nv); what was missing
// was the prelude, and about that there was not one word. The user is sent to
// hunt for a mistake in a file that has none.
//
// The trigger is not exotic: our own quickstart warns that without the leading dot
// in `. ./setup-env.ps1` the variables never get set, so this is the EXPECTED
// beginner mistake, met in the first five minutes of following our own guide.
//
// The differential that made it a defect rather than a preference: the import
// resolver answers the SAME deficit loudly -- `cannot find module 'std.time.duration'
// ... searched: <three paths>`. One deficit, two opposite reactions.
//
// Both sides are asserted here, because a fixture that only proves the refusal
// would pass just as happily on a compiler that refuses everything.

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

/// A directory OUTSIDE the repository holding the quickstart hello-world, so that
/// walking up from it finds no `std/` and no `nova.toml`.
fn quickstart_dir(tag: &str) -> PathBuf {
    let mut dir = std::env::temp_dir();
    dir.push(format!("nova_prelude_822_{}_{}", std::process::id(), tag));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("mkdir temp dir");
    fs::write(
        dir.join("hello.nv"),
        "module hello\n\nfn main() Io -> () {\n    println(\"hi\")\n}\n",
    )
    .expect("write hello.nv");
    dir
}

#[test]
fn without_the_prelude_the_compiler_names_its_own_deficit() {
    let dir = quickstart_dir("missing");
    let out = nova()
        .arg("check")
        .arg("hello.nv")
        .current_dir(&dir)
        .env_remove("NOVA_STD_PATH")
        .output()
        .expect("run nova check");
    let all = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

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
    assert!(
        all.contains("prelude.nv"),
        "the searched list must name the file, not just the directory; got:\n{}",
        all
    );
    // The heart of the row: the user's correct source must NOT be the accused.
    assert!(
        !all.contains("undefined identifier `println`"),
        "the user's own correct line must not be blamed for our missing std; got:\n{}",
        all
    );
}

#[test]
fn with_the_prelude_the_same_file_compiles() {
    let dir = quickstart_dir("present");
    let out = nova()
        .arg("check")
        .arg("hello.nv")
        .current_dir(&dir)
        .env("NOVA_STD_PATH", repo_std())
        .output()
        .expect("run nova check");
    assert_eq!(
        out.status.code(),
        Some(0),
        "the same file, with std reachable, must pass; stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}
