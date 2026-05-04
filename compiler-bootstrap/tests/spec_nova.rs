//! Запускает все .nv-файлы из ../tests-nova/ через интерпретатор и
//! проверяет, что все `test "..." { ... }` блоки внутри проходят.
//!
//! tests-nova/ живёт на top-level репозитория, не в compiler-bootstrap/ —
//! это conformance tests языка, общие для любого компилятора Nova.
//!
//! Это «тестируем язык на самом языке» — bootstrap должен поддержать
//! каждую заявленную фичу, иначе соответствующий test-блок упадёт.

use nova::interp::Interpreter;
use nova::parser::parse;
use std::fs;
use std::path::PathBuf;

fn run_nova_test_file(path: &PathBuf) -> (usize, usize, Vec<String>) {
    let src = fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("failed to read {}: {}", path.display(), e));
    let module = parse(&src).unwrap_or_else(|d| {
        panic!(
            "parse error in {}: {}\n--- src ---\n{}",
            path.display(),
            d.message,
            src
        )
    });
    let mut interp = Interpreter::new();
    interp
        .load_module(&module)
        .unwrap_or_else(|d| panic!("load error in {}: {}", path.display(), d.message));
    interp
        .run_tests()
        .unwrap_or_else(|d| panic!("run error in {}: {}", path.display(), d.message))
}

fn collect_nova_files() -> Vec<PathBuf> {
    // tests-nova/ — на top-level репо (sibling to compiler-bootstrap/).
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("CARGO_MANIFEST_DIR has no parent")
        .join("tests-nova");
    let mut files: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("failed to read {}: {}", dir.display(), e))
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|p| p.extension().map(|e| e == "nv").unwrap_or(false))
        .collect();
    files.sort();
    files
}

#[test]
fn all_spec_test_files_pass() {
    let files = collect_nova_files();
    if files.is_empty() {
        panic!("no .nv files found in tests-nova/");
    }
    let mut total_passed = 0;
    let mut total_failed = 0;
    let mut failure_lines: Vec<String> = Vec::new();
    for f in &files {
        let (passed, failed, names) = run_nova_test_file(f);
        let name = f.file_name().unwrap().to_string_lossy();
        if failed > 0 {
            failure_lines.push(format!("{}: {} failed", name, failed));
            for n in &names {
                failure_lines.push(format!("  FAIL: {} :: {}", name, n));
            }
        }
        eprintln!(
            "{:<35} {} passed, {} failed",
            name, passed, failed
        );
        total_passed += passed;
        total_failed += failed;
    }
    eprintln!("---");
    eprintln!(
        "TOTAL: {} files, {} passed, {} failed",
        files.len(),
        total_passed,
        total_failed
    );
    if total_failed > 0 {
        panic!("\n{} Nova-test(s) failed:\n{}", total_failed, failure_lines.join("\n"));
    }
}
