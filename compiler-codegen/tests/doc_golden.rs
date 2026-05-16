//! Plan 45 Ф.19 — golden-snapshot regression tests для doc-fixtures.
//!
//! Для каждой fixture `nova_tests/doc/fixtures/<name>/sample.nv`:
//! - парсим + строим DocTree;
//! - рендерим в JSON через `render_json_with_source`;
//! - сравниваем с committed `expected.json`.
//!
//! Расхождение → test fails. Перегенерировать snapshot вручную:
//! `nova doc <fixture>/sample.nv --format json > <fixture>/expected.json`
//! (только после подтверждения, что изменение output'а намеренное).
//!
//! Test использует относительные пути от crate root; работает в
//! release builds (см. CI workflow `.github/workflows/nova-doc.yml`).

use std::path::PathBuf;

fn fixtures_root() -> PathBuf {
    // CARGO_MANIFEST_DIR указывает на `compiler-codegen/`.
    // Fixtures в `<repo>/nova_tests/doc/fixtures/`.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("compiler-codegen has parent")
        .join("nova_tests/doc/fixtures")
}

fn run_one(fixture_name: &str) {
    let dir = fixtures_root().join(fixture_name);
    let src_path = dir.join("sample.nv");
    let expected_path = dir.join("expected.json");
    let src = std::fs::read_to_string(&src_path)
        .unwrap_or_else(|e| panic!("read {}: {}", src_path.display(), e));

    let regen = std::env::var("REGEN").is_ok();

    if !regen && !expected_path.exists() {
        panic!("expected.json missing for fixture `{}`. Run with REGEN=1 to generate.", fixture_name);
    }

    let expected = if regen {
        String::new()
    } else {
        std::fs::read_to_string(&expected_path)
            .unwrap_or_else(|e| panic!("read {}: {}", expected_path.display(), e))
    };

    let mut module = nova_codegen::parser::parse(&src)
        .unwrap_or_else(|d| panic!("parse {}: {}", src_path.display(), d.message));
    let _ = nova_codegen::types::check_module(&module);
    nova_codegen::types::infer_effects(&mut module);
    let mut tree = nova_codegen::doc::build(&module);
    nova_codegen::doc::strip_private(&mut tree);
    let actual = nova_codegen::doc::render_json_with_source(&tree, &src);

    // Normalize line-endings (Windows CRLF в expected.json не должен
    // ломать тест на Linux CI).
    let actual_norm = actual.replace("\r\n", "\n");
    let expected_norm = expected.replace("\r\n", "\n");

    if regen {
        std::fs::write(&expected_path, actual.as_bytes())
            .unwrap_or_else(|e| panic!("write {}: {}", expected_path.display(), e));
        eprintln!("REGEN: wrote {}", expected_path.display());
        return;
    }

    if actual_norm != expected_norm {
        // Print compact diff hint.
        eprintln!("=== fixture {} JSON mismatch ===", fixture_name);
        eprintln!("--- expected ({} bytes) ---", expected_norm.len());
        eprintln!("--- actual   ({} bytes) ---", actual_norm.len());
        for (i, (a, e)) in actual_norm
            .lines()
            .zip(expected_norm.lines())
            .enumerate()
        {
            if a != e {
                eprintln!("line {}:", i + 1);
                eprintln!("  expected: {}", e);
                eprintln!("  actual:   {}", a);
                break;
            }
        }
        panic!(
            "fixture `{}`: JSON output drift. Run tests with REGEN=1 to regenerate if intentional.",
            fixture_name
        );
    }
}

#[test]
fn golden_basic() {
    run_one("basic");
}

#[test]
fn golden_sections() {
    run_one("sections");
}

#[test]
fn golden_kinds() {
    run_one("kinds");
}

#[test]
fn golden_links() {
    run_one("links");
}

#[test]
fn golden_orphan() {
    run_one("orphan");
}

#[test]
fn golden_doctests() {
    run_one("doctests");
}

#[test]
fn golden_stability() {
    run_one("stability");
}

#[test]
fn golden_real_attrs() {
    run_one("real_attrs");
}

#[test]
fn golden_module_attrs() {
    run_one("module_attrs");
}

#[test]
fn golden_should_panic() {
    run_one("should_panic");
}

#[test]
fn golden_must_verify() {
    run_one("must_verify");
}

#[test]
fn golden_capability_forbid() {
    run_one("capability_forbid");
}

#[test]
fn golden_expect_output() {
    run_one("expect_output");
}
