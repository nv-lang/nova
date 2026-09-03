// Registry 821 -- the two refusals of ONE door, and the door itself.
//
// The rule "a Nova entry point is a `.nv` file" used to live as a COPIED LINE in
// eight commands, and the copy is why it was simultaneously missing and too strict:
//
//   (a) `cmd_doc` had no copy at all, so `nova doc hello.txt` exited 0 and printed an
//       empty document -- a silent success on garbage input, which this project treats
//       as the worst possible outcome (class 770);
//   (b) every copy compared the extension case-SENSITIVELY, so `nova check HELLO.NV`
//       was refused while `nova check hello.nv` on the SAME file (case-insensitive
//       filesystem) succeeded -- the answer depended on how the argument was typed.
//
// These tests drive the real binary, because what is being asserted is a CLI contract
// (exit code plus message), not the behaviour of an internal function.
//
// Isolation per test is mandatory and not tidiness: Nova reads a directory as ONE
// module of co-equal files, so a shared temp directory would fuse these fixtures into
// one module and collide their `fn main`.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn nova() -> Command {
    Command::new(env!("CARGO_BIN_EXE_nova"))
}

fn isolated_dir(tag: &str) -> PathBuf {
    let mut dir = std::env::temp_dir();
    dir.push(format!("nova_ext_door_{}_{}", std::process::id(), tag));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("mkdir temp dir");
    dir
}

const HELLO: &str = "module hello

fn main() Io -> () {
    println(\"hi\")
}
";

/// (a) The refusal that did not exist: `doc` on a file that is not Nova source.
/// Exit 2 is the project's usage-error code -- the same one `check` already returned
/// for the same input, which is the whole point of a single door.
#[test]
fn doc_refuses_a_file_that_is_not_nova_source() {
    let dir = isolated_dir("doc_txt");
    let f = dir.join("hello.txt");
    fs::write(&f, "hello
").expect("write");

    let out = nova().arg("doc").arg(&f).output().expect("run nova doc");
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert_eq!(
        out.status.code(),
        Some(2),
        "`nova doc` on a .txt must be a usage error, not a silent success; stderr: {}",
        stderr
    );
    assert!(
        stderr.contains("not a Nova source"),
        "the refusal must name the reason; stderr: {}",
        stderr
    );
}

/// (b) The refusal that should never have existed: an uppercase extension.
/// The file is literally named `.NV` (not the same file reached through a different
/// spelling) so the test means the same thing on a case-sensitive filesystem.
#[test]
fn an_uppercase_extension_is_accepted() {
    let dir = isolated_dir("upper");
    let f = dir.join("UPPER.NV");
    fs::write(&f, HELLO).expect("write");

    let out = nova().arg("check").arg(&f).output().expect("run nova check");
    assert_eq!(
        out.status.code(),
        Some(0),
        "`.NV` names the same kind of file as `.nv`; stdout: {} stderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

/// The boundary: relaxing the case comparison must not relax the RULE. A genuinely
/// wrong extension is still refused, or the fix would have traded one silent success
/// for another.
#[test]
fn a_wrong_extension_is_still_refused() {
    let dir = isolated_dir("still_refused");
    let f = dir.join("hello.txt");
    fs::write(&f, HELLO).expect("write");

    let out = nova().arg("check").arg(&f).output().expect("run nova check");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(2), "stderr: {}", stderr);
    assert!(stderr.contains("not a Nova source"), "stderr: {}", stderr);
}
