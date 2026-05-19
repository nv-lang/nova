//! Plan 62.F.bis Ф.1 — integration tests для edition versioning prelude
//! resolution.
//!
//! Verifies два пути:
//!   1. `[package].edition = "2026.05"` → resolver выбирает
//!      `std/prelude/e2026_05.nv` (с `PRELUDE_EDITION_2026_05` маркером).
//!   2. Без edition → resolver fallback на rolling `std/prelude.nv` facade
//!      (с `PRELUDE_VERSION` маркером).
//!
//! Каждый тест создаёт ephemeral workspace в `target/edition_fixture_*/`,
//! копирует туда минимальный `nova.toml` + entry-файл, запускает `nova
//! check` указывая на entry. Test assertions — на stdout/stderr `nova
//! check` (success + лог).

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("nova-cli has a parent dir")
        .to_path_buf()
}

/// Создаёт ephemeral workspace в `target/<name>/` и возвращает path
/// к `src/main.nv`. Caller передаёт content для nova.toml + main.nv.
fn make_workspace(name: &str, nova_toml: &str, main_nv: &str) -> PathBuf {
    let repo = repo_root();
    let ws_root = repo.join("target").join(name);
    // Clean any prior run.
    let _ = fs::remove_dir_all(&ws_root);
    fs::create_dir_all(ws_root.join("src")).expect("mkdir src/");
    fs::write(ws_root.join("nova.toml"), nova_toml).expect("write nova.toml");
    let main_path = ws_root.join("src").join("main.nv");
    fs::write(&main_path, main_nv).expect("write main.nv");
    main_path
}

#[test]
fn edition_2026_05_pin_resolves_marker() {
    let main_path = make_workspace(
        "edition_fixture_2026_05",
        r#"
[package]
name = "edition_test"
edition = "2026.05"

[lib]
src = "src"
"#,
        r#"
module edition_test.main

// PRELUDE_EDITION_2026_05 определён ТОЛЬКО в std/prelude/e2026_05.nv.
// Если resolver выбрал rolling facade — symbol будет unknown → check fail.
fn pin_marker() -> int {
    PRELUDE_EDITION_2026_05
}

test "edition 2026.05 pin marker visible" {
    let n = pin_marker()
    assert(n == 5)
}
"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_nova"))
        .arg("check")
        .arg(&main_path)
        .current_dir(repo_root())
        .output()
        .expect("failed to spawn `nova check`");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "nova check should succeed for edition pin workspace.\n\
         stdout: {}\nstderr: {}",
        stdout, stderr,
    );
}

#[test]
fn no_edition_falls_back_to_rolling_facade() {
    let main_path = make_workspace(
        "edition_fixture_no_edition",
        r#"
[package]
name = "rolling_test"

[lib]
src = "src"
"#,
        r#"
module rolling_test.main

// PRELUDE_VERSION определён в rolling `std/prelude.nv` facade. Если
// resolver bug'нулся (например wrong edition path) — symbol будет
// unknown.
fn rolling_marker() -> int {
    PRELUDE_VERSION
}

test "rolling facade marker visible" {
    let n = rolling_marker()
    assert(n == 5)
}
"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_nova"))
        .arg("check")
        .arg(&main_path)
        .current_dir(repo_root())
        .output()
        .expect("failed to spawn `nova check`");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "nova check should succeed for no-edition workspace (rolling facade).\n\
         stdout: {}\nstderr: {}",
        stdout, stderr,
    );
}

#[test]
fn unknown_edition_falls_back_to_rolling() {
    // edition specified, но соответствующий pin файл не существует →
    // soft-fall back на rolling facade (PRELUDE_VERSION остаётся видимым).
    let main_path = make_workspace(
        "edition_fixture_unknown",
        r#"
[package]
name = "unknown_edition_test"
edition = "9999.99"

[lib]
src = "src"
"#,
        r#"
module unknown_edition_test.main

// Никакого e9999_99.nv pin'а нет → resolver falls back на rolling
// `std/prelude.nv`. PRELUDE_VERSION визибл (proves fallback worked).
fn marker() -> int {
    PRELUDE_VERSION
}

test "unknown edition falls back to rolling" {
    assert(marker() == 5)
}
"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_nova"))
        .arg("check")
        .arg(&main_path)
        .current_dir(repo_root())
        .output()
        .expect("failed to spawn `nova check`");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "unknown edition should silently fall back to rolling facade.\n\
         stdout: {}\nstderr: {}",
        stdout, stderr,
    );
}
