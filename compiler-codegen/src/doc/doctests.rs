//! Plan 45 Ф.7 — doc-tests collector.
//!
//! Извлекает ` ```nova ` fenced code blocks из doc-content'а (summary +
//! description + sections) каждого item'а и module'а. Поддерживает
//! info-string modifiers (по D104 §«Doc-test modifiers»):
//!
//! - `nova` — обычный test, компилируется + запускается;
//! - `nova,no_run` — компилируется, не запускается;
//! - `nova,ignore` — не компилируется (только для отображения);
//! - `nova,compile_fail` — ожидается compile-error;
//! - `nova,should_panic` — ожидается runtime panic;
//! - `nova,must_verify` — ожидается successful SMT verification (Plan 33).
//!
//! Несколько модификаторов разделяются запятыми: `nova,no_run,ignore`.
//! Неизвестные модификаторы — игнорируются (forward-compat).
//!
//! **Hidden lines** (по rustdoc convention): строки, начинающиеся с
//! `# ` или `#` (без текста), скрыты при рендеринге, но включены при
//! компиляции. Это позволяет писать boilerplate (imports, module
//! declaration) без захламления визуального примера.
//!
//! Этот pass только **собирает** doc-tests в `DocTree.doc_tests`;
//! выполнение — отдельный subcommand `nova doc --test` (Plan 45 Ф.14)
//! или test-runner integration (Plan 28).

use super::doctree::*;

pub fn collect_doc_tests(tree: &mut DocTree) {
    let mut found: Vec<DocTest> = Vec::new();
    let mut counter: u32 = 0;
    let mut warnings: Vec<DocWarning> = Vec::new();
    for m in &tree.modules {
        // Module-level tests.
        let module_id = format!("<module:{}>", m.path.join("."));
        for text in [m.summary.as_deref(), m.description.as_deref()]
            .into_iter()
            .flatten()
        {
            extract_from_text(text, None, &m.path.join("."), &mut counter, None, &mut found, &module_id, &mut warnings);
        }
        for it in &m.items {
            let mut texts: Vec<&str> = Vec::new();
            texts.extend(it.summary.as_deref());
            texts.extend(it.description.as_deref());
            for v in it.sections.values() {
                texts.push(v.as_str());
            }
            for text in texts {
                extract_from_text(
                    text,
                    Some(it.id.clone()),
                    &m.path.join("."),
                    &mut counter,
                    it.doc_test_handlers.as_deref(),
                    &mut found,
                    &it.id,
                    &mut warnings,
                );
            }
        }
    }
    // Deterministic order: по (from_id, index).
    found.sort_by(|a, b| {
        a.from_id
            .cmp(&b.from_id)
            .then(a.index.cmp(&b.index))
    });
    tree.doc_tests = found;
    tree.warnings.extend(warnings);
    tree.warnings.sort();
    tree.warnings.dedup();
}

fn extract_from_text(
    text: &str,
    from_id: Option<String>,
    module_path: &str,
    counter: &mut u32,
    test_handlers: Option<&str>,
    out: &mut Vec<DocTest>,
    item_id: &str,
    warnings: &mut Vec<DocWarning>,
) {
    let mut lines = text.lines().peekable();
    while let Some(line) = lines.next() {
        // Ищем открывающую fence: ```nova ... или ``` nova ... или ```nova,modifiers
        let trimmed = line.trim_start();
        let info = match strip_fence_open(trimmed) {
            Some(s) => s,
            None => continue,
        };
        if !is_nova_fence(info) {
            // ``` без `nova` — не doc-test (просто generic-code пример).
            // Скипуем до закрывающего ```.
            for l in lines.by_ref() {
                if l.trim_start().starts_with("```") {
                    break;
                }
            }
            continue;
        }
        let modifiers = parse_modifiers(info, item_id, warnings);
        let has_expect_output = modifiers.contains(&DocTestModifier::ExpectOutput);
        // Считываем тело до закрывающего ```.
        let mut visible = String::new();
        let mut full = String::new();
        // Plan 45 Ф.24.8: accumulate `// Output: <text>` lines when expect_output.
        let mut output_lines: Vec<String> = Vec::new();
        for l in lines.by_ref() {
            let t = l.trim_start();
            if t.starts_with("```") {
                break;
            }
            // Plan 45 Ф.24.8: `// Output: <text>` — expected stdout line.
            // These appear in the fenced block but are NOT compiled — they are
            // metadata extracted here, visible in render but excluded from full_source.
            if has_expect_output {
                if let Some(expected) = t.strip_prefix("// Output:") {
                    output_lines.push(expected.trim().to_string());
                    // Still include in visible (godoc shows them), skip from full_source.
                    visible.push_str(l);
                    visible.push('\n');
                    continue;
                }
            }
            // Hidden lines: `# ` (rustdoc) → в full, не в visible.
            if let Some(hidden_content) = strip_hidden_prefix(l) {
                full.push_str(hidden_content);
                full.push('\n');
            } else {
                visible.push_str(l);
                visible.push('\n');
                full.push_str(l);
                full.push('\n');
            }
        }
        // Build expected_output: join lines with '\n'; None if no Output: annotations found.
        let expected_output = if has_expect_output && !output_lines.is_empty() {
            Some(output_lines.join("\n"))
        } else {
            None
        };
        *counter += 1;
        out.push(DocTest {
            id: format!("{}::doc_test_{}", module_path, *counter),
            from_id: from_id.clone(),
            index: *counter,
            modifiers,
            visible_source: visible.trim_end_matches('\n').to_string(),
            full_source: full.trim_end_matches('\n').to_string(),
            test_handlers: test_handlers.map(|s| s.to_string()),
            expected_output,
        });
    }
}

fn strip_fence_open(line: &str) -> Option<&str> {
    line.strip_prefix("```")
}

/// `info` — содержимое после открывающего ```. Может быть:
/// - `nova` или `nova,no_run` — doc-test;
/// - `nova ` (с пробелом) — также doc-test;
/// - `rust`, `text`, `` (empty) — не doc-test.
fn is_nova_fence(info: &str) -> bool {
    let lang = info
        .split(|c: char| c == ',' || c.is_whitespace())
        .next()
        .unwrap_or("");
    lang == "nova"
}

fn parse_modifiers(info: &str, item_id: &str, warnings: &mut Vec<DocWarning>) -> Vec<DocTestModifier> {
    let mut out = Vec::new();
    // info = "nova,no_run,ignore" → берём всё после первой запятой.
    let rest = match info.split_once(',') {
        Some((_, r)) => r,
        None => return out,
    };
    for tok in rest.split(',') {
        let t = tok.trim();
        if t.is_empty() { continue; }
        let m = match t {
            "no_run" => Some(DocTestModifier::NoRun),
            "ignore" => Some(DocTestModifier::Ignore),
            "compile_fail" => Some(DocTestModifier::CompileFail),
            "should_panic" => Some(DocTestModifier::ShouldPanic),
            "must_verify" => Some(DocTestModifier::MustVerify),
            "expect_output" => Some(DocTestModifier::ExpectOutput),
            "infer_contracts" => Some(DocTestModifier::InferContracts),
            _ => None,
        };
        match m {
            Some(m) => out.push(m),
            None => {
                // Plan 45 Ф.25.1: было silent forward-compat skip.
                // Теперь — warning. Skip остаётся (forward-compat сохранён).
                warnings.push(DocWarning {
                    rule: "unknown-doctest-modifier".to_string(),
                    item_id: item_id.to_string(),
                    message: format!(
                        "unknown doc-test modifier `{}` in fence info `{}` — ignored (forward-compat). Known: no_run, ignore, compile_fail, should_panic, must_verify, expect_output, infer_contracts",
                        t, info
                    ),
                });
            }
        }
    }
    out
}

/// Rustdoc convention: строки `# code` или `#code` (без `## ` markdown
/// heading) — hidden boilerplate. Возвращает `Some(content без префикса)`,
/// если строка hidden.
fn strip_hidden_prefix(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    if let Some(rest) = trimmed.strip_prefix("# ") {
        return Some(rest);
    }
    // `#` в начале строки без пробела — hidden empty line.
    if trimmed == "#" {
        return Some("");
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_item(id: &str, doc_summary: &str, doc_desc: Option<&str>) -> DocItem {
        DocItem {
            id: id.to_string(),
            module_path: vec!["m".to_string()],
            name: id.split("::").last().unwrap().to_string(),
            visibility: Visibility::Export,
            summary: Some(doc_summary.to_string()),
            description: doc_desc.map(String::from),
            sections: Default::default(),
            deprecation: None,
            stability: None,
            aliases: Vec::new(),
            hide_doc: false,
            doc_test_handlers: None,
            kind: ItemKind::Const {
                ty: "int".to_string(),
                value: "0".to_string(),
            },
            source_span: crate::diag::Span {
                start: 0,
                end: 0,
                file_id: 0,
            },
            peer_file: None,
            linked_from: Vec::new(),
            capabilities: Default::default(),
            reexport_from: None,
            doc_inline: false,
            scraped_examples: Vec::new(),
        }
    }

    fn make_tree(items: Vec<DocItem>) -> DocTree {
        let mut t = DocTree::new();
        t.modules.push(DocModule {
            path: vec!["m".to_string()],
            name: "m".to_string(),
            kind: ModuleKind::File,
            peers: vec![],
            summary: None,
            description: None,
            deprecation: None,
            stability: None,
            hide_doc: false,
            items,
            effect_matrix: Vec::new(),
            realtime_matrix: Vec::new(),
            source_span: crate::diag::Span {
                start: 0,
                end: 0,
                file_id: 0,
            },
            source_paths: Vec::new(),
        });
        t
    }

    #[test]
    fn extracts_simple_nova_block() {
        let item = make_item("m::f", "Demo.", Some("```nova\nlet x = 1\n```"));
        let mut tree = make_tree(vec![item]);
        collect_doc_tests(&mut tree);
        assert_eq!(tree.doc_tests.len(), 1);
        assert_eq!(tree.doc_tests[0].visible_source, "let x = 1");
        assert!(tree.doc_tests[0].modifiers.is_empty());
    }

    #[test]
    fn ignores_non_nova_blocks() {
        let item = make_item("m::f", "Demo.", Some("```rust\nlet x = 1\n```"));
        let mut tree = make_tree(vec![item]);
        collect_doc_tests(&mut tree);
        assert_eq!(tree.doc_tests.len(), 0);
    }

    #[test]
    fn parses_modifiers() {
        let item = make_item(
            "m::f",
            "Demo.",
            Some("```nova,no_run,should_panic\npanic\n```"),
        );
        let mut tree = make_tree(vec![item]);
        collect_doc_tests(&mut tree);
        assert_eq!(tree.doc_tests.len(), 1);
        assert!(tree.doc_tests[0]
            .modifiers
            .contains(&DocTestModifier::NoRun));
        assert!(tree.doc_tests[0]
            .modifiers
            .contains(&DocTestModifier::ShouldPanic));
    }

    #[test]
    fn unknown_modifier_skipped_with_warning() {
        // Plan 45 Ф.25.1: unknown modifier раньше silently skip,
        // теперь — skip + warning в tree.warnings.
        let item = make_item("m::f", "Demo.", Some("```nova,wat\nlet x = 1\n```"));
        let mut tree = make_tree(vec![item]);
        collect_doc_tests(&mut tree);
        assert_eq!(tree.doc_tests.len(), 1);
        assert!(tree.doc_tests[0].modifiers.is_empty());
        // Warning generated.
        assert_eq!(tree.warnings.len(), 1);
        assert_eq!(tree.warnings[0].rule, "unknown-doctest-modifier");
        assert_eq!(tree.warnings[0].item_id, "m::f");
        assert!(tree.warnings[0].message.contains("wat"));
    }

    #[test]
    fn hidden_lines_excluded_from_visible() {
        let item = make_item(
            "m::f",
            "Demo.",
            Some("```nova\n# import std.io\nlet x = 1\n```"),
        );
        let mut tree = make_tree(vec![item]);
        collect_doc_tests(&mut tree);
        let t = &tree.doc_tests[0];
        assert_eq!(t.visible_source, "let x = 1");
        assert!(t.full_source.contains("import std.io"));
        assert!(t.full_source.contains("let x = 1"));
    }

    #[test]
    fn multiple_blocks_one_item() {
        let item = make_item(
            "m::f",
            "Demo.",
            Some("```nova\nlet x = 1\n```\n\n```nova\nlet y = 2\n```"),
        );
        let mut tree = make_tree(vec![item]);
        collect_doc_tests(&mut tree);
        assert_eq!(tree.doc_tests.len(), 2);
    }
}
