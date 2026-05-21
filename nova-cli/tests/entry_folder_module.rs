//! Plan 81 Ф.10 — integration-тест: entry-folder-module компилируется.
//!
//! Когда сам компилируемый entry-файл — peer folder-module, resolver
//! (`resolve_imports_inline_ex`) обязан собрать sibling peers и
//! проверить группу как один модуль. До Ф.10 caller парсил entry как
//! один файл (`MAIN_FILE_ID`) и sibling peers не собирались вовсе —
//! `[M-entry-folder-module]` в docs/simplifications.md.
//!
//! Тест запускает реальный `nova` binary (`CARGO_BIN_EXE_nova`) в
//! режиме `check` на peer'е `app.nv` folder-модуля
//! `nova_tests/plan81/entry_fmod/`. `app.nv` вызывает `helper`,
//! объявленную в соседнем peer'е `lib.nv`. Без Ф.10 `lib.nv` не
//! собирался бы → `helper` undefined → `nova check` падает.

use std::path::PathBuf;
use std::process::Command;

/// Корень репозитория — `nova-cli/` родитель.
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("nova-cli has a parent dir")
        .to_path_buf()
}

#[test]
fn nova_check_entry_folder_module_peer() {
    let repo = repo_root();
    let entry = repo
        .join("nova_tests")
        .join("plan81")
        .join("entry_fmod")
        .join("app.nv");
    assert!(
        entry.is_file(),
        "fixture missing: {} — Plan 81 Ф.10 regression fixture",
        entry.display()
    );

    let output = Command::new(env!("CARGO_BIN_EXE_nova"))
        .arg("check")
        .arg(&entry)
        .current_dir(&repo)
        .output()
        .expect("failed to spawn `nova check`");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "`nova check` on an entry-folder-module peer failed.\n\
         Without Plan 81 Ф.10 the sibling peer `lib.nv` is not collected, \
         so `helper` is undefined in `app.nv`.\n\
         stdout: {}\nstderr: {}",
        stdout,
        stderr,
    );
}
