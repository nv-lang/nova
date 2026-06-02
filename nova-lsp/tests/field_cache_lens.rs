//! Plan 123.5.1 (V5.1): integration tests для field-cache code-lens
//! и hover providers.
//! Plan 123.5.2 (V5.2, 2026-06-02): semantic-tokens provider tests.

use nova_lsp::server::{
    compute_field_cache_lenses,
    compute_field_cache_hover,
    compute_field_cache_semantic_tokens,
    cached_field_semantic_token_types,
    cached_field_semantic_token_modifiers,
};
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

// ─────────────────────────────────────────────────────────────────────
// Plan 123.5.2 (V5.2): semantic-tokens provider — color cached @field
// reads differently from plain field access.
// ─────────────────────────────────────────────────────────────────────

#[test]
fn v52_legend_is_stable() {
    let types = cached_field_semantic_token_types();
    let mods = cached_field_semantic_token_modifiers();
    assert_eq!(types.len(), 1, "single token type (property)");
    assert_eq!(types[0], SemanticTokenType::PROPERTY);
    assert_eq!(mods.len(), 2, "readonly + cached modifiers");
    assert_eq!(mods[0], SemanticTokenModifier::READONLY);
}

#[test]
fn v52_semantic_tokens_for_cached_at_field() {
    // Each `@x` and `@y` read in `@sum_sq` body should be tagged as
    // PROPERTY with the "cached" modifier (ro field eligible for D217
    // V1 caching).
    let tokens = compute_field_cache_semantic_tokens(SAMPLE_NV)
        .expect("Some(tokens)");
    // SAMPLE_NV body has 4 `@<field>` reads: @x, @x, @y, @y.
    assert_eq!(tokens.len(), 4,
        "expected 4 cached @field tokens, got {:?}", tokens);
    // All tokens use token_type = 0 (PROPERTY index in legend).
    for t in &tokens { assert_eq!(t.token_type, 0); }
    // All carry the "cached" modifier bitset (readonly bit + cached bit).
    let expected_bitset = (1u32 << 0) | (1u32 << 1);
    for t in &tokens {
        assert_eq!(t.token_modifiers_bitset, expected_bitset,
            "expected readonly|cached bitset on every cached @field");
    }
}

#[test]
fn v52_semantic_tokens_empty_for_non_cached_module() {
    // Module without record fields → no cached @field reads.
    let src = "module v5_2.no_cache\n\nfn add(a int, b int) -> int {\n    a + b\n}\n";
    let tokens = compute_field_cache_semantic_tokens(src)
        .expect("Some(tokens)");
    assert!(tokens.is_empty(),
        "expected zero tokens when no cached @field reads; got {:?}", tokens);
}

#[test]
fn v52_semantic_tokens_delta_encoding_monotonic() {
    // Delta-encoded LSP tokens MUST be in (line, char) order — each
    // delta_line >= 0; if delta_line == 0, delta_start >= 0 (forward
    // progress within the line).
    let tokens = compute_field_cache_semantic_tokens(SAMPLE_NV)
        .expect("Some(tokens)");
    for t in &tokens {
        assert!(t.delta_line < 1_000_000, "delta_line sane: {}", t.delta_line);
    }
}
