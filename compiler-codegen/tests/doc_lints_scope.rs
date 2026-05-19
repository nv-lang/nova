// SPDX-License-Identifier: MIT OR Apache-2.0
//! Plan 71 / D127 — `public-missing-stability` scope tests.
//!
//! Покрывает acceptance criteria плана 71 Ф.5 (6 тестов):
//!
//! 1. `default_warning_non_strict` — user .nv без flag, exported item
//!    без `#stable` → severity = Warning (exit 0 в CLI).
//! 2. `strict_error_with_flag` — manifest `enforce-stability = true`,
//!    тот же код → severity = Error (exit 1).
//! 3. `fixture_exempt_default` — file под `nova_tests/`, default config
//!    → lint skip (нет violation).
//! 4. `fixture_exempt_strict` — file под `nova_tests/`, strict config →
//!    skip remains (test-exemption берёт верх над flag'ом).
//! 5. `examples_exempt` — file под `examples/` → skip.
//! 6. `bench_exempt` — file под `bench/corpus/` → skip.
//!
//! Тесты строят `DocModule` напрямую (без полного parse/build) — это
//! изолирует логику severity + fixture detection от смежных pass'ов.
//! Дополнительный sanity test проверяет workspace-уровень integration
//! (`doc::build` → `source_paths` правильно populated через collector).
//!
//! Spec source-of-truth: `spec/decisions/09-tooling.md#d127-stability-tier-enforcement-scope`.

use std::path::PathBuf;

use nova_codegen::doc::doctree::{
    DocItem, DocModule, DocTree, ItemKind, ModuleKind, Signature, VerifyStatus, Visibility,
};
use nova_codegen::doc::{run_lints, LintConfig, Severity};
use nova_codegen::diag::Span;

// ------------ helpers ------------

/// Минимальный exported fn без stability — мишень для
/// `public-missing-stability` (item-level rule 7).
fn make_export_fn(id: &str, name: &str, module_path: &[&str]) -> DocItem {
    let module_path: Vec<String> = module_path.iter().map(|s| s.to_string()).collect();
    DocItem {
        id: id.to_string(),
        module_path,
        name: name.to_string(),
        visibility: Visibility::Export,
        summary: Some("Compute foo.".to_string()),
        description: None,
        sections: std::collections::BTreeMap::new(),
        deprecation: None,
        stability: None,           // ← главное: нет stability tier
        hide_doc: false,
        kind: ItemKind::Fn(Signature {
            receiver: None,
            generics: Vec::new(),
            params: Vec::new(),
            return_type: "int".to_string(),
            effects: Vec::new(),
            raises: Vec::new(),
            contracts: Vec::new(),
            verify_status: VerifyStatus::NotAttempted,
        }),
        aliases: Vec::new(),
        doc_test_handlers: None,
        source_span: zero_span(),
        peer_file: None,
        linked_from: Vec::new(),
        capabilities: Default::default(),
        reexport_from: None,
        doc_inline: false,
        scraped_examples: Vec::new(),
    }
}

/// Build a `DocModule` с одним exported fn (без stability) и заданными
/// source-путями. `path_parts` — module path; `source_paths` — список
/// абсолютных/относительных путей для fixture-detection.
fn make_module(
    path_parts: &[&str],
    source_paths: Vec<PathBuf>,
    include_unstable_export_fn: bool,
) -> DocModule {
    let path: Vec<String> = path_parts.iter().map(|s| s.to_string()).collect();
    let name = path.last().cloned().unwrap_or_default();
    let items = if include_unstable_export_fn {
        // Item ID: `<module>::<name>`.
        let id = format!("{}::foo", path.join("."));
        vec![make_export_fn(&id, "foo", path_parts)]
    } else {
        Vec::new()
    };
    DocModule {
        path,
        name,
        kind: ModuleKind::File,
        peers: Vec::new(),
        summary: None,
        description: None,
        deprecation: None,
        stability: None,            // ← module-level тоже без stability
        hide_doc: false,
        items,
        effect_matrix: Vec::new(),
        realtime_matrix: Vec::new(),
        source_span: zero_span(),
        source_paths,
    }
}

fn zero_span() -> Span {
    Span {
        start: 0,
        end: 0,
        file_id: 0,
    }
}

/// Helper: построить DocTree с одним module.
fn tree_with(module: DocModule) -> DocTree {
    let mut t = DocTree::new();
    t.modules.push(module);
    t
}

/// Helper: фильтрует violations по rule = "public-missing-stability".
fn stability_violations(
    tree: &DocTree,
    config: &LintConfig,
) -> Vec<nova_codegen::doc::DocLintViolation> {
    run_lints(tree, config)
        .into_iter()
        .filter(|v| v.rule == "public-missing-stability")
        .collect()
}

// ------------ Plan 71 Ф.5 acceptance: 6 tests ------------

/// Ф.5 №1: default config (без enforce-stability), user .nv path → Warning.
///
/// User code (не под fixture-dir) без flag'а — lint emit'ит warning на
/// module-level и item-level. CLI exit 0 (warnings не блокируют без --strict).
#[test]
fn default_warning_non_strict() {
    let module = make_module(
        &["my_app", "src", "foo"],
        vec![PathBuf::from("my_app/src/foo.nv")],
        true,
    );
    let tree = tree_with(module);
    let config = LintConfig::default();

    let violations = stability_violations(&tree, &config);
    assert!(
        !violations.is_empty(),
        "default config должен emit'ить warning(s) для exported item без stability"
    );
    for v in &violations {
        assert_eq!(
            v.severity,
            Severity::Warning,
            "default config: severity ДОЛЖЕН быть Warning, не Error (violation: {})",
            v.message
        );
    }
}

/// Ф.5 №2: enforce-stability=true → Error.
///
/// Тот же код, но с `LintConfig.strict_stability = true` (mapping
/// `manifest.enforce_stability`). Severity должна быть Error → CLI exit 1.
#[test]
fn strict_error_with_flag() {
    let module = make_module(
        &["my_app", "src", "foo"],
        vec![PathBuf::from("my_app/src/foo.nv")],
        true,
    );
    let tree = tree_with(module);
    let config = LintConfig {
        strict_stability: true,
        fixture_dirs: LintConfig::default_fixture_dirs(),
    };

    let violations = stability_violations(&tree, &config);
    assert!(
        !violations.is_empty(),
        "strict mode должен emit'ить error(s)"
    );
    for v in &violations {
        assert_eq!(
            v.severity,
            Severity::Error,
            "strict mode: severity ДОЛЖЕН быть Error (violation: {})",
            v.message
        );
    }
}

/// Ф.5 №3: file под `nova_tests/`, default config → skip полностью.
///
/// Test fixtures — не public API surface, даже warning не должен
/// emit'иться.
#[test]
fn fixture_exempt_default() {
    let module = make_module(
        &["nova_tests", "doc", "fixtures", "basic", "sample"],
        vec![PathBuf::from(
            "nova_tests/doc/fixtures/basic/sample.nv",
        )],
        true,
    );
    let tree = tree_with(module);
    let config = LintConfig::default();

    let violations = stability_violations(&tree, &config);
    assert!(
        violations.is_empty(),
        "fixture path ДОЛЖЕН быть exempt от public-missing-stability \
         (default config), но получили: {:?}",
        violations.iter().map(|v| &v.message).collect::<Vec<_>>()
    );
}

/// Ф.5 №4: file под `nova_tests/`, strict config → still skip.
///
/// Test exemption берёт верх над `enforce-stability = true`. Даже в
/// stdlib (где flag включён), test fixtures под `nova_tests/` остаются
/// silent.
#[test]
fn fixture_exempt_strict() {
    let module = make_module(
        &["nova_tests", "doc", "fixtures", "basic", "sample"],
        vec![PathBuf::from(
            "nova_tests/doc/fixtures/basic/sample.nv",
        )],
        true,
    );
    let tree = tree_with(module);
    let config = LintConfig {
        strict_stability: true,
        fixture_dirs: LintConfig::default_fixture_dirs(),
    };

    let violations = stability_violations(&tree, &config);
    assert!(
        violations.is_empty(),
        "fixture exemption ДОЛЖЕН перевешивать strict flag, но получили: {:?}",
        violations.iter().map(|v| &v.message).collect::<Vec<_>>()
    );
}

/// Ф.5 №5: file под `examples/` → skip.
#[test]
fn examples_exempt() {
    let module = make_module(
        &["examples", "hello", "world"],
        vec![PathBuf::from("examples/hello/world.nv")],
        true,
    );
    let tree = tree_with(module);

    // Тестируем оба config'а — exemption применяется независимо.
    for strict in [false, true] {
        let config = LintConfig {
            strict_stability: strict,
            fixture_dirs: LintConfig::default_fixture_dirs(),
        };
        let violations = stability_violations(&tree, &config);
        assert!(
            violations.is_empty(),
            "examples/ exempt должен работать (strict={}); got: {:?}",
            strict,
            violations.iter().map(|v| &v.message).collect::<Vec<_>>()
        );
    }
}

/// Ф.5 №6: file под `bench/corpus/` → skip.
#[test]
fn bench_exempt() {
    let module = make_module(
        &["bench", "corpus", "perf"],
        vec![PathBuf::from("bench/corpus/perf.nv")],
        true,
    );
    let tree = tree_with(module);

    for strict in [false, true] {
        let config = LintConfig {
            strict_stability: strict,
            fixture_dirs: LintConfig::default_fixture_dirs(),
        };
        let violations = stability_violations(&tree, &config);
        assert!(
            violations.is_empty(),
            "bench/ exempt должен работать (strict={}); got: {:?}",
            strict,
            violations.iter().map(|v| &v.message).collect::<Vec<_>>()
        );
    }
}

// ------------ Plan 71 Ф.5 extras (robustness) ------------

/// Robustness: путь `tests/` (без `nova_` префикса) тоже exempt — это
/// стандартное Rust-style location для integration tests user crate'а.
#[test]
fn tests_dir_exempt() {
    let module = make_module(
        &["myapp", "tests", "integration"],
        vec![PathBuf::from("myapp/tests/integration.nv")],
        true,
    );
    let tree = tree_with(module);
    let config = LintConfig::default();
    let violations = stability_violations(&tree, &config);
    assert!(
        violations.is_empty(),
        "tests/ exempt должен работать, но получили: {:?}",
        violations.iter().map(|v| &v.message).collect::<Vec<_>>()
    );
}

/// Robustness: Windows backslash paths — fixture detection должен
/// работать через `Path::components()` regardless of separator.
#[test]
fn windows_separator_fixture_detection() {
    let module = make_module(
        &["nova_tests", "doc", "fixtures", "basic", "sample"],
        vec![PathBuf::from(
            r"D:\Sources\nv-lang\nova\nova_tests\doc\fixtures\basic\sample.nv",
        )],
        true,
    );
    let tree = tree_with(module);
    let config = LintConfig::default();
    let violations = stability_violations(&tree, &config);
    assert!(
        violations.is_empty(),
        "Windows path должен правильно резолвиться через components(), \
         но получили: {:?}",
        violations.iter().map(|v| &v.message).collect::<Vec<_>>()
    );
}

/// Robustness: пустой `source_paths` (synthesized tree без peer_files) →
/// fixture detection возвращает false (fallthrough к строгому пути).
#[test]
fn empty_source_paths_not_fixture() {
    let module = make_module(&["synthetic"], Vec::new(), true);
    let tree = tree_with(module);

    // Default config — Warning emit.
    let config = LintConfig::default();
    let violations = stability_violations(&tree, &config);
    assert!(
        !violations.is_empty(),
        "synthesized module (без source_paths) должен по-прежнему линтиться"
    );
    for v in &violations {
        assert_eq!(v.severity, Severity::Warning);
    }
}

/// Robustness: module без exported items и без module-stability → если
/// fixture, exempt; иначе module-level warning. Проверка отрицательного
/// path: stdlib-style path БЕЗ fixture dir → emit Warning module-level.
#[test]
fn stdlib_path_lints_module_level() {
    let module = make_module(
        &["std", "encoding", "base64"],
        vec![PathBuf::from("std/encoding/base64.nv")],
        false, // нет item'ов; только module-level lint
    );
    let tree = tree_with(module);
    let config = LintConfig::default(); // strict=false
    let violations = stability_violations(&tree, &config);
    assert_eq!(
        violations.len(),
        1,
        "module-level warning ожидается"
    );
    assert_eq!(violations[0].severity, Severity::Warning);
    assert!(violations[0].message.contains("public module has no stability"));
}

/// Sanity: другие правила (broken/missing-summary не покрыты этим test
/// suite'ом; они emit'ятся всегда как Error). Здесь проверяем что
/// severity-аккуратность работает на `examples-missing` (всегда Error).
#[test]
fn other_rules_remain_error_severity() {
    // export fn без `# Examples` секции — emit `examples-missing` Error.
    let module = make_module(
        &["app", "lib"],
        vec![PathBuf::from("app/lib.nv")],
        true,
    );
    let tree = tree_with(module);
    let config = LintConfig::default();
    let violations: Vec<_> = run_lints(&tree, &config)
        .into_iter()
        .filter(|v| v.rule == "examples-missing")
        .collect();
    assert!(!violations.is_empty());
    for v in &violations {
        assert_eq!(
            v.severity,
            Severity::Error,
            "other rules должны оставаться Error severity (не downgrade'ятся)"
        );
    }
}
