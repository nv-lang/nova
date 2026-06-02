//! Plan 123.5.1 (V5.1): integration tests для field-cache code-lens
//! и hover providers.

use nova_lsp::server::{compute_field_cache_lenses, compute_field_cache_hover};
use tower_lsp::lsp_types::*;

const SAMPLE_NV: &str = "module v5_1.sample

type Vec {
    ro x int
    ro y int
}

fn Vec @sum_sq() -> int {
    @x * @x + @y * @y
}
";

#[test]
fn code_lens_reports_cached_methods() {
    let lenses = compute_field_cache_lenses(SAMPLE_NV).expect("Some(lenses)");
    assert!(!lenses.is_empty(), "expected at least 1 code-lens for sum_sq");

    let titles: Vec<String> = lenses.iter()
        .filter_map(|l| l.command.as_ref().map(|c| c.title.clone()))
        .collect();
    let any_with_ro = titles.iter().any(|t| t.contains("ro=") && !t.contains("ro=0"));
    assert!(any_with_ro, "expected ro caches > 0; titles: {:?}", titles);
}

#[test]
fn hover_returns_cache_info_for_at_field() {
    // Position pointing к `@x` (column 5 на line 8).
    let pos = Position { line: 8, character: 5 };
    let hover = compute_field_cache_hover(SAMPLE_NV, pos);
    if let Some(h) = hover {
        if let HoverContents::Scalar(MarkedString::String(s)) = h.contents {
            assert!(s.contains("Plan 123 field-cache"));
        } else {
            panic!("expected scalar marked string");
        }
    }
}

#[test]
fn hover_outside_at_field_returns_none() {
    // Position на whitespace.
    let pos = Position { line: 0, character: 0 };
    let hover = compute_field_cache_hover(SAMPLE_NV, pos);
    assert!(hover.is_none(), "expected None for module keyword position");
}
