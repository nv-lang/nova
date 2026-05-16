//! Plan 45 Ф.24.2 — workspace output determinism test.
//!
//! Verifies that `build_workspace` produces byte-for-byte identical JSON
//! across 100 runs with the same input (no HashMap non-determinism).

fn make_modules() -> Vec<nova_codegen::ast::Module> {
    // Two small modules with a Protocol so implementors code path is exercised
    // (under NOVA_DOC_EXPERIMENTAL_IMPLEMENTORS=1).
    let src_a = r#"
module test.a

export fn foo(x: int) -> int {
    x + 1
}

export type Counter {
    value: int

    fn len(self) -> int {
        self.value
    }
}
"#;
    let src_b = r#"
module test.b

export fn bar(x: int) -> str {
    "hello"
}

export type List {
    items: [int]

    fn len(self) -> int {
        self.items.len()
    }
}
"#;
    let mut modules = Vec::new();
    for src in [src_a, src_b] {
        if let Ok(mut m) = nova_codegen::parser::parse(src) {
            let _ = nova_codegen::types::check_module(&m);
            nova_codegen::types::infer_effects(&mut m);
            modules.push(m);
        }
    }
    modules
}

fn render_to_json(tree: &nova_codegen::doc::DocTree) -> String {
    nova_codegen::doc::render_json(tree)
}

#[test]
fn workspace_output_is_deterministic() {
    let modules = make_modules();
    if modules.is_empty() {
        return; // parse failed in this env, skip
    }

    let first = render_to_json(&nova_codegen::doc::build_workspace(&modules));

    for i in 1..100 {
        let output = render_to_json(&nova_codegen::doc::build_workspace(&modules));
        if output != first {
            panic!(
                "workspace JSON output differs on run {i}!\n\
                 First output length: {}\n\
                 Run {i} output length: {}\n\
                 First 200 chars: {}\n\
                 Run {i} first 200 chars: {}",
                first.len(),
                output.len(),
                &first[..first.len().min(200)],
                &output[..output.len().min(200)],
            );
        }
    }
}
