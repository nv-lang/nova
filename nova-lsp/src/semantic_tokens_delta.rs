// SPDX-License-Identifier: MIT OR Apache-2.0
//! Plan 123.5.5 (V5.5, 2026-06-03) — incremental LSP semantic-tokens delta.
//!
//! LSP `textDocument/semanticTokens/full/delta` request shape — server
//! returns `SemanticTokensFullDeltaResult::TokensDelta { result_id, edits }`
//! describing how to transform a previously-served full token set into
//! the new one. Client passes back the `previous_result_id` it last
//! received; server validates it матчит ranks last cached snapshot, и
//! computes a minimal edit script.
//!
//! Edit algorithm: **single-edit prefix-suffix reduction**. Find the
//! longest matching prefix and suffix at the SemanticToken-level, emit
//! ONE `SemanticTokensEdit` that replaces the middle. Result indices
//! are in flat-u32 space (start/delete_count = token_count × 5).
//!
//! This is not minimum-edit (true LCS would be optimal) but produces
//! one-edit-per-change which is bandwidth-equivalent для typical
//! incremental edits (single-cursor insert/delete localized к one
//! region). Worst-case (interleaved changes) collapses к single-edit
//! whole-tail replacement — still wire-equivalent к full-token fallback.
//!
//! Pure module — no IO, no LSP runtime. State (`SemanticTokensSnapshot`
//! cache) lives в `WorkspaceState`; this file is the algorithm.

use tower_lsp::lsp_types::{
    SemanticToken, SemanticTokens, SemanticTokensDelta, SemanticTokensEdit,
    SemanticTokensFullDeltaResult,
};

/// Per-document cached snapshot — the last full token set served and
/// its `result_id`. Used by the server to validate delta requests and
/// to compute the edit script against the latest tokens.
#[derive(Debug, Clone)]
pub struct SemanticTokensSnapshot {
    pub result_id: String,
    pub tokens: Vec<SemanticToken>,
}

/// Compute minimal-prefix-suffix single-edit script transforming `old`
/// → `new`. Returns empty vec when `old == new` (no-op delta).
///
/// Indices in returned [`SemanticTokensEdit`] are in **flat-u32 space**:
/// each `SemanticToken` occupies 5 u32s on the wire (deltaLine,
/// deltaStart, length, tokenType, tokenModifiersBitset). Therefore
/// `start` and `delete_count` are always multiples of 5.
///
/// Guarantees:
/// - `old == new` → returns `Vec::new()` (zero edits).
/// - Pure suffix change → one edit at end.
/// - Pure prefix change → one edit at start.
/// - Middle change → one edit spanning altered middle.
/// - Total replacement (no shared prefix/suffix) → one edit covering
///   the entire range.
pub fn compute_semantic_token_edits(
    old: &[SemanticToken],
    new: &[SemanticToken],
) -> Vec<SemanticTokensEdit> {
    if old == new {
        return Vec::new();
    }

    let prefix_tokens: usize = old
        .iter()
        .zip(new.iter())
        .take_while(|(a, b)| semantic_tokens_equal(a, b))
        .count();

    let remaining_old = old.len() - prefix_tokens;
    let remaining_new = new.len() - prefix_tokens;
    let max_suffix = remaining_old.min(remaining_new);

    let mut suffix_tokens = 0usize;
    while suffix_tokens < max_suffix
        && semantic_tokens_equal(
            &old[old.len() - 1 - suffix_tokens],
            &new[new.len() - 1 - suffix_tokens],
        )
    {
        suffix_tokens += 1;
    }

    let start_u32 = (prefix_tokens * 5) as u32;
    let delete_count_u32 = ((old.len() - prefix_tokens - suffix_tokens) * 5) as u32;
    let inserted_slice = &new[prefix_tokens..new.len() - suffix_tokens];
    let data = if inserted_slice.is_empty() {
        None
    } else {
        Some(inserted_slice.to_vec())
    };

    vec![SemanticTokensEdit {
        start: start_u32,
        delete_count: delete_count_u32,
        data,
    }]
}

/// Plan 123.5.5 (V5.5): Decision-and-snapshot helper для server handler.
/// Encapsulates the pure logic of `semantic_tokens_full_delta`:
///
/// * If the cached snapshot's `result_id` matches the client's
///   `prev_result_id`, return `TokensDelta { result_id: new, edits }`.
/// * Otherwise (no cache OR stale id) fall back к returning the full
///   token set as `Tokens { result_id: new, data }`.
///
/// In **both cases** returns the **fresh snapshot** that the caller
/// must insert into the per-URI cache — keeps server-side state-update
/// consistent and testable in isolation.
pub fn build_delta_response(
    prev_snapshot: Option<&SemanticTokensSnapshot>,
    prev_result_id: &str,
    new_tokens: Vec<SemanticToken>,
    new_result_id: String,
) -> (SemanticTokensFullDeltaResult, SemanticTokensSnapshot) {
    match prev_snapshot {
        Some(snap) if snap.result_id == prev_result_id => {
            let edits = compute_semantic_token_edits(&snap.tokens, &new_tokens);
            let updated = SemanticTokensSnapshot {
                result_id: new_result_id.clone(),
                tokens: new_tokens,
            };
            (
                SemanticTokensFullDeltaResult::TokensDelta(SemanticTokensDelta {
                    result_id: Some(new_result_id),
                    edits,
                }),
                updated,
            )
        }
        _ => {
            let updated = SemanticTokensSnapshot {
                result_id: new_result_id.clone(),
                tokens: new_tokens.clone(),
            };
            (
                SemanticTokensFullDeltaResult::Tokens(SemanticTokens {
                    result_id: Some(new_result_id),
                    data: new_tokens,
                }),
                updated,
            )
        }
    }
}

/// Equality predicate для two semantic tokens — `SemanticToken` derives
/// `PartialEq` via lsp-types, но keep an explicit predicate so future
/// fuzzy match (e.g. ignore delta_line/delta_start for re-encoding
/// scenarios) lands в one place.
#[inline]
fn semantic_tokens_equal(a: &SemanticToken, b: &SemanticToken) -> bool {
    a.delta_line == b.delta_line
        && a.delta_start == b.delta_start
        && a.length == b.length
        && a.token_type == b.token_type
        && a.token_modifiers_bitset == b.token_modifiers_bitset
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp::lsp_types::SemanticToken;

    fn tok(delta_line: u32, delta_start: u32, length: u32) -> SemanticToken {
        SemanticToken {
            delta_line,
            delta_start,
            length,
            token_type: 0,
            token_modifiers_bitset: 0,
        }
    }

    // ── V5.5.1 positive: identical → no-op ───────────────────────────

    #[test]
    fn v5_5_identical_input_emits_zero_edits() {
        let old = vec![tok(1, 0, 2), tok(1, 1, 2)];
        let new = old.clone();
        let edits = compute_semantic_token_edits(&old, &new);
        assert!(edits.is_empty(), "expected zero edits для identical input");
    }

    // ── V5.5.2 positive: append at end ───────────────────────────────

    #[test]
    fn v5_5_append_token_at_end_emits_single_tail_edit() {
        let old = vec![tok(1, 0, 2), tok(1, 1, 2)];
        let mut new = old.clone();
        new.push(tok(1, 2, 3));
        let edits = compute_semantic_token_edits(&old, &new);
        assert_eq!(edits.len(), 1);
        let e = &edits[0];
        assert_eq!(e.start, 10, "append should start at flat-end (2 tokens × 5)");
        assert_eq!(e.delete_count, 0);
        assert_eq!(e.data.as_ref().map(|v| v.len()), Some(1));
        assert_eq!(e.data.as_ref().unwrap()[0], tok(1, 2, 3));
    }

    // ── V5.5.3 positive: prepend at start ────────────────────────────

    #[test]
    fn v5_5_prepend_token_emits_single_head_edit() {
        let old = vec![tok(1, 0, 2), tok(1, 1, 2)];
        let mut new = vec![tok(0, 0, 1)];
        new.extend(old.iter().cloned());
        let edits = compute_semantic_token_edits(&old, &new);
        assert_eq!(edits.len(), 1);
        let e = &edits[0];
        assert_eq!(e.start, 0);
        assert_eq!(e.delete_count, 0);
        assert_eq!(e.data.as_ref().map(|v| v.len()), Some(1));
        assert_eq!(e.data.as_ref().unwrap()[0], tok(0, 0, 1));
    }

    // ── V5.5.4 positive: middle replacement ──────────────────────────

    #[test]
    fn v5_5_middle_change_emits_single_middle_edit() {
        let old = vec![tok(0, 0, 1), tok(1, 0, 2), tok(2, 0, 3)];
        let new = vec![tok(0, 0, 1), tok(1, 5, 4), tok(2, 0, 3)];
        let edits = compute_semantic_token_edits(&old, &new);
        assert_eq!(edits.len(), 1);
        let e = &edits[0];
        // Common prefix = 1 token (5 u32s); common suffix = 1 token.
        assert_eq!(e.start, 5, "start = 1 token prefix × 5");
        assert_eq!(e.delete_count, 5, "delete middle 1 token × 5");
        assert_eq!(e.data.as_ref().map(|v| v.len()), Some(1));
        assert_eq!(e.data.as_ref().unwrap()[0], tok(1, 5, 4));
    }

    // ── V5.5.5 positive: tail deletion ───────────────────────────────

    #[test]
    fn v5_5_tail_deletion_emits_zero_data_edit() {
        let old = vec![tok(1, 0, 2), tok(1, 1, 2), tok(1, 2, 3)];
        let new = vec![tok(1, 0, 2)];
        let edits = compute_semantic_token_edits(&old, &new);
        assert_eq!(edits.len(), 1);
        let e = &edits[0];
        assert_eq!(e.start, 5, "start after first kept token");
        assert_eq!(e.delete_count, 10, "delete 2 tail tokens × 5");
        assert!(e.data.is_none(), "tail delete должен иметь None data");
    }

    // ── V5.5.6 positive: total replacement (no shared prefix/suffix) ─

    #[test]
    fn v5_5_total_replacement_emits_single_full_edit() {
        let old = vec![tok(1, 0, 2), tok(1, 5, 3)];
        let new = vec![tok(2, 0, 4), tok(2, 5, 5)];
        let edits = compute_semantic_token_edits(&old, &new);
        assert_eq!(edits.len(), 1);
        let e = &edits[0];
        assert_eq!(e.start, 0);
        assert_eq!(e.delete_count, 10);
        assert_eq!(e.data.as_ref().map(|v| v.len()), Some(2));
    }

    // ── V5.5.7 positive: empty old, non-empty new (initial cache) ────

    #[test]
    fn v5_5_empty_old_emits_pure_insertion() {
        let old: Vec<SemanticToken> = Vec::new();
        let new = vec![tok(1, 0, 2)];
        let edits = compute_semantic_token_edits(&old, &new);
        assert_eq!(edits.len(), 1);
        let e = &edits[0];
        assert_eq!(e.start, 0);
        assert_eq!(e.delete_count, 0);
        assert_eq!(e.data.as_ref().map(|v| v.len()), Some(1));
    }

    // ── V5.5.8 positive: non-empty old, empty new (clearing) ─────────

    #[test]
    fn v5_5_empty_new_emits_full_deletion() {
        let old = vec![tok(1, 0, 2), tok(1, 1, 3)];
        let new: Vec<SemanticToken> = Vec::new();
        let edits = compute_semantic_token_edits(&old, &new);
        assert_eq!(edits.len(), 1);
        let e = &edits[0];
        assert_eq!(e.start, 0);
        assert_eq!(e.delete_count, 10);
        assert!(e.data.is_none());
    }

    // ── V5.5.9 negative: token_modifiers difference detected ─────────

    #[test]
    fn v5_5_modifier_difference_treated_as_changed() {
        // Same delta_line/start/length/type, different modifiers bitset
        // → должен trigger middle edit, не считать tokens equal.
        let old_t = SemanticToken {
            delta_line: 1, delta_start: 0, length: 2,
            token_type: 0, token_modifiers_bitset: 0b01,
        };
        let new_t = SemanticToken {
            delta_line: 1, delta_start: 0, length: 2,
            token_type: 0, token_modifiers_bitset: 0b11,
        };
        let edits = compute_semantic_token_edits(&[old_t], &[new_t]);
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].start, 0);
        assert_eq!(edits[0].delete_count, 5);
    }

    // ── V5.5.10 invariant: indices always 5-aligned ──────────────────

    #[test]
    fn v5_5_indices_are_5_aligned_invariant() {
        let old = vec![tok(0, 0, 1), tok(1, 0, 2), tok(2, 0, 3), tok(3, 0, 4)];
        let new = vec![tok(0, 0, 1), tok(1, 5, 9)];
        let edits = compute_semantic_token_edits(&old, &new);
        for e in &edits {
            assert_eq!(e.start % 5, 0, "start must be 5-aligned");
            assert_eq!(e.delete_count % 5, 0, "delete_count must be 5-aligned");
        }
    }

    // ── V5.5.11 positive: matching prev id → TokensDelta variant ─────

    #[test]
    fn v5_5_matching_prev_id_emits_tokens_delta() {
        let snap = SemanticTokensSnapshot {
            result_id: "st-7".to_string(),
            tokens: vec![tok(1, 0, 2)],
        };
        let new_tokens = vec![tok(1, 0, 2), tok(1, 3, 4)];
        let (response, updated) = build_delta_response(
            Some(&snap),
            "st-7",
            new_tokens.clone(),
            "st-8".to_string(),
        );
        match response {
            SemanticTokensFullDeltaResult::TokensDelta(d) => {
                assert_eq!(d.result_id, Some("st-8".to_string()));
                assert_eq!(d.edits.len(), 1, "single tail-edit expected");
                assert_eq!(d.edits[0].start, 5);
                assert_eq!(d.edits[0].delete_count, 0);
            }
            _ => panic!("expected TokensDelta variant"),
        }
        assert_eq!(updated.result_id, "st-8");
        assert_eq!(updated.tokens, new_tokens, "snapshot must reflect new tokens");
    }

    // ── V5.5.12 negative: mismatched prev id → fallback к full ───────

    #[test]
    fn v5_5_mismatched_prev_id_falls_back_to_full_tokens() {
        let snap = SemanticTokensSnapshot {
            result_id: "st-99".to_string(),
            tokens: vec![tok(1, 0, 2)],
        };
        let new_tokens = vec![tok(2, 0, 5)];
        let (response, updated) = build_delta_response(
            Some(&snap),
            "st-0", // stale id
            new_tokens.clone(),
            "st-100".to_string(),
        );
        match response {
            SemanticTokensFullDeltaResult::Tokens(t) => {
                assert_eq!(t.result_id, Some("st-100".to_string()));
                assert_eq!(t.data, new_tokens);
            }
            _ => panic!("expected Tokens fallback variant для stale prev_id"),
        }
        assert_eq!(updated.result_id, "st-100", "snapshot id refreshed");
        assert_eq!(updated.tokens, new_tokens);
    }

    // ── V5.5.13 negative: no cached snapshot → fallback к full ───────

    #[test]
    fn v5_5_no_snapshot_falls_back_to_full_tokens() {
        let new_tokens = vec![tok(0, 0, 3)];
        let (response, updated) = build_delta_response(
            None,
            "st-any",
            new_tokens.clone(),
            "st-1".to_string(),
        );
        match response {
            SemanticTokensFullDeltaResult::Tokens(t) => {
                assert_eq!(t.result_id, Some("st-1".to_string()));
                assert_eq!(t.data, new_tokens);
            }
            _ => panic!("expected Tokens fallback variant when no cache"),
        }
        assert_eq!(updated.result_id, "st-1");
    }

    // ── V5.5.14 positive: matching id + identical tokens → empty edits

    #[test]
    fn v5_5_matching_id_identical_tokens_emits_empty_edits() {
        let tokens = vec![tok(1, 0, 2), tok(1, 1, 3)];
        let snap = SemanticTokensSnapshot {
            result_id: "st-42".to_string(),
            tokens: tokens.clone(),
        };
        let (response, updated) = build_delta_response(
            Some(&snap),
            "st-42",
            tokens.clone(),
            "st-43".to_string(),
        );
        match response {
            SemanticTokensFullDeltaResult::TokensDelta(d) => {
                assert!(d.edits.is_empty(),
                    "identical tokens должны давать zero edits");
                assert_eq!(d.result_id, Some("st-43".to_string()));
            }
            _ => panic!("expected TokensDelta variant"),
        }
        assert_eq!(updated.tokens, tokens);
    }
}
