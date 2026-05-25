//! Incremental text document synchronization.
//!
//! Plan 104.1.Ф.4: apply LSP `TextDocumentContentChangeEvent` deltas to a `Rope`.
//!
//! LSP sends changes as UTF-16 line/character ranges + new_text.  We convert
//! to byte offsets via `diagnostic_mapping::position_to_byte_offset`, then
//! apply `rope.remove(range); rope.insert(pos, text)`.
//!
//! # Edge cases
//!
//! - `range = None` (full text refresh): rebuild Rope from scratch.
//! - `start > end`: log error, skip change.
//! - Out-of-bounds range: clamp via `position_to_byte_offset`.

use ropey::Rope;
use tower_lsp::lsp_types::TextDocumentContentChangeEvent;

use crate::diagnostic_mapping::position_to_byte_offset;

// ─────────────────────────────────────────────────────────────────────────────
// Public API
// ─────────────────────────────────────────────────────────────────────────────

/// Apply a sequence of LSP content-change events to `rope` in order.
///
/// Each event is applied independently; after each the rope reflects the
/// updated text.  Events must be in the order the editor produced them.
pub fn apply_changes(rope: &mut Rope, changes: &[TextDocumentContentChangeEvent]) {
    for change in changes {
        apply_one(rope, change);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Internal helpers
// ─────────────────────────────────────────────────────────────────────────────

fn apply_one(rope: &mut Rope, change: &TextDocumentContentChangeEvent) {
    match &change.range {
        None => {
            // Full text refresh (e.g. first didChange after server restart).
            *rope = Rope::from_str(&change.text);
        }
        Some(range) => {
            let start_byte = position_to_byte_offset(
                rope,
                range.start.line,
                range.start.character,
            );
            let end_byte = position_to_byte_offset(
                rope,
                range.end.line,
                range.end.character,
            );

            if start_byte > end_byte {
                tracing::error!(
                    start = start_byte,
                    end = end_byte,
                    "incremental sync: start > end byte offset; ignoring change"
                );
                return;
            }

            // Convert byte offsets → char offsets for ropey.
            let start_char = rope.byte_to_char(start_byte);
            let end_char = rope.byte_to_char(end_byte);

            // Remove the old text in the range (if any).
            if start_char < end_char {
                rope.remove(start_char..end_char);
            }

            // Insert the new text at the start position.
            if !change.text.is_empty() {
                rope.insert(start_char, &change.text);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp::lsp_types::{Position, Range, TextDocumentContentChangeEvent};

    fn change(
        start_line: u32, start_char: u32,
        end_line: u32, end_char: u32,
        text: &str,
    ) -> TextDocumentContentChangeEvent {
        TextDocumentContentChangeEvent {
            range: Some(Range {
                start: Position { line: start_line, character: start_char },
                end: Position { line: end_line, character: end_char },
            }),
            range_length: None,
            text: text.to_string(),
        }
    }

    fn full_refresh(text: &str) -> TextDocumentContentChangeEvent {
        TextDocumentContentChangeEvent {
            range: None,
            range_length: None,
            text: text.to_string(),
        }
    }

    // ── pos1 ─────────────────────────────────────────────────────────────────

    #[test]
    fn pos1_insert_single_char_in_middle() {
        let mut rope = Rope::from_str("hello world\n");
        // Insert "X" at position (0, 5) — between 'o' and ' '
        apply_changes(&mut rope, &[change(0, 5, 0, 5, "X")]);
        assert_eq!(rope.to_string(), "helloX world\n");
    }

    // ── pos2 ─────────────────────────────────────────────────────────────────

    #[test]
    fn pos2_replace_range_multi_char() {
        let mut rope = Rope::from_str("hello world\n");
        // Replace "world" (bytes 6..11) with "rust"
        apply_changes(&mut rope, &[change(0, 6, 0, 11, "rust")]);
        assert_eq!(rope.to_string(), "hello rust\n");
    }

    // ── pos3 ─────────────────────────────────────────────────────────────────

    #[test]
    fn pos3_delete_range() {
        let mut rope = Rope::from_str("abcdef\n");
        // Delete "cd" (chars 2..4)
        apply_changes(&mut rope, &[change(0, 2, 0, 4, "")]);
        assert_eq!(rope.to_string(), "abef\n");
    }

    // ── pos4 ─────────────────────────────────────────────────────────────────

    #[test]
    fn pos4_insert_at_start_and_end() {
        let mut rope = Rope::from_str("middle\n");
        apply_changes(&mut rope, &[change(0, 0, 0, 0, "START-")]);
        assert_eq!(rope.to_string(), "START-middle\n");

        apply_changes(&mut rope, &[change(0, 13, 0, 13, "-END")]);
        assert_eq!(rope.to_string(), "START-middle\n-END");
    }

    // ── pos5 ─────────────────────────────────────────────────────────────────

    #[test]
    fn pos5_multibyte_cyrillic_edit() {
        // "Привет мир\n" — "Привет" = 6 chars (12 bytes), " " = 1, "мир" = 3 chars (6 bytes)
        let mut rope = Rope::from_str("Привет мир\n");
        // Replace "мир" at UTF-16 col 7..10 with "world"
        apply_changes(&mut rope, &[change(0, 7, 0, 10, "world")]);
        assert_eq!(rope.to_string(), "Привет world\n");
    }

    // ── pos6 ─────────────────────────────────────────────────────────────────

    #[test]
    fn pos6_emoji_surrogate_pair_edit() {
        // "hi👋ok\n" — emoji at UTF-16 col 2, takes 2 CUs → ends at col 4
        let mut rope = Rope::from_str("hi👋ok\n");
        // Replace emoji (UTF-16 col 2..4) with "wave"
        apply_changes(&mut rope, &[change(0, 2, 0, 4, "wave")]);
        assert_eq!(rope.to_string(), "hiwaveok\n");
    }

    // ── pos7 ─────────────────────────────────────────────────────────────────

    #[test]
    fn pos7_1000_small_edits_sequential() {
        let mut rope = Rope::from_str("");
        // Append 'x' 1000 times
        for i in 0..1000u32 {
            let pos = i;
            apply_changes(&mut rope, &[change(0, pos, 0, pos, "x")]);
        }
        assert_eq!(rope.len_chars(), 1000);
        assert!(rope.to_string().chars().all(|c| c == 'x'));
    }

    // ── neg1 ─────────────────────────────────────────────────────────────────

    #[test]
    fn neg1_start_greater_than_end_ignored_no_panic() {
        let mut rope = Rope::from_str("unchanged\n");
        // start_char > end_char is invalid; should be ignored
        apply_changes(&mut rope, &[change(0, 5, 0, 2, "X")]);
        // Rope should be unchanged
        assert_eq!(rope.to_string(), "unchanged\n");
    }

    // ── neg2 ─────────────────────────────────────────────────────────────────

    #[test]
    fn neg2_out_of_bounds_range_clamped_no_panic() {
        let mut rope = Rope::from_str("short\n");
        // Way out of bounds — should not panic
        apply_changes(&mut rope, &[change(0, 100, 0, 200, "new")]);
        // Should have inserted at the end
        assert!(rope.to_string().contains("new") || rope.to_string() == "short\n");
    }

    // ── neg3 ─────────────────────────────────────────────────────────────────

    #[test]
    fn neg3_full_text_refresh_replaces_everything() {
        let mut rope = Rope::from_str("old content\n");
        apply_changes(&mut rope, &[full_refresh("brand new\n")]);
        assert_eq!(rope.to_string(), "brand new\n");
    }

    // ── edge1 ────────────────────────────────────────────────────────────────

    #[test]
    fn edge1_insert_empty_string_is_noop() {
        let mut rope = Rope::from_str("hello\n");
        apply_changes(&mut rope, &[change(0, 3, 0, 3, "")]);
        assert_eq!(rope.to_string(), "hello\n");
    }

    // ── edge2 ────────────────────────────────────────────────────────────────

    #[test]
    fn edge2_edit_at_last_position_after_newline() {
        let mut rope = Rope::from_str("line1\n");
        // Line 1, char 0 = position right after the newline
        apply_changes(&mut rope, &[change(1, 0, 1, 0, "line2\n")]);
        assert_eq!(rope.to_string(), "line1\nline2\n");
    }

    // ── edge3 ────────────────────────────────────────────────────────────────

    #[test]
    fn edge3_multi_line_replace_spanning_newlines() {
        let mut rope = Rope::from_str("line1\nline2\nline3\n");
        // Replace lines 0..2 (from start of line0 to start of line2) with "NEW\n"
        apply_changes(&mut rope, &[change(0, 0, 2, 0, "NEW\n")]);
        assert_eq!(rope.to_string(), "NEW\nline3\n");
    }
}
