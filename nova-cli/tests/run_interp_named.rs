//! Plan 50 Ф.2 — integration-тест treewalk-интерпретатора (`nova run`)
//! для именованных аргументов. Регрессия на [M-interp-named]
//! (RESOLVED): `cmd_run` делает `resolve_imports_inline` + `callnorm`
//! перед интерпретацией, поэтому переставленные named args для
//! ИМПОРТИРОВАННОГО callee раскладываются в param-order корректно.
//!
//! Тест запускает реальный `nova` binary (`CARGO_BIN_EXE_nova`) на
//! фикстуре `nova_tests/named_params/imported_named_run.nv` и проверяет
//! stdout. Эта фикстура также прогоняется codegen-suite — двойное
//! покрытие обоих путей (interp + codegen).

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
fn nova_run_reordered_named_args_imported_callee() {
    let repo = repo_root();
    let fixture = repo
        .join("nova_tests")
        .join("named_params")
        .join("imported_named_run.nv");
    assert!(
        fixture.is_file(),
        "fixture missing: {} — Plan 50 Ф.2 regression fixture",
        fixture.display()
    );

    let output = Command::new(env!("CARGO_BIN_EXE_nova"))
        .arg("run")
        .arg(&fixture)
        .current_dir(&repo)
        .output()
        .expect("failed to spawn `nova run`");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "`nova run` exited non-zero.\nstdout: {}\nstderr: {}",
        stdout,
        stderr,
    );

    // configure(name, retries=3, verbose=false) = name.len + retries*10 + (verbose?1000:0)
    //   configure("ab")                          → 32
    //   configure("ab", verbose:true, retries:5) → 1052  (reordered named)
    //   configure("abc", retries:2)              → 23
    //
    // Если interp не раскладывал бы reorder — второй вызов дал бы
    // verbose=5, retries=true → неверное число. Точное совпадение
    // подтверждает корректную param-order раскладку.
    assert!(
        stdout.contains("p50-interp 32 1052 23"),
        "interp produced wrong result for reordered named args on imported \
         callee.\nstdout: {}\nstderr: {}",
        stdout,
        stderr,
    );
}
