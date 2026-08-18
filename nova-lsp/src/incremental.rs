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
    // ── prop ─────────────────────────────────────────────────────────────────
    //
    // THE INVARIANT THIS FILE EXISTS FOR, and it had no test until 2026-08-18:
    // after ANY sequence of incremental edits, the server's copy of the document
    // must equal the editor's, byte for byte. Every position the server ever
    // reports -- a diagnostic, a hover, an inlay hint -- is an offset INTO that
    // copy. If it drifts, positions stay internally consistent and become
    // externally wrong, which is why the drift is invisible from inside: the
    // hints are right for a text nobody is looking at.
    //
    // The per-case tests above check individual edits against hand-written
    // expectations. That cannot catch drift, because drift needs a SEQUENCE:
    // each edit is applied to the result of the previous one, so one bad offset
    // silently poisons every edit after it.
    //
    // Reference implementation is deliberately INDEPENDENT -- a plain String and
    // a hand-rolled LSP position walker (lines split on \n, \r\n and lone \r;
    // characters counted in UTF-16 code units, per the LSP spec). Comparing
    // ropey against ropey would only prove it agrees with itself.

    /// Deterministic generator: a fixed sequence beats a random one that cannot
    /// be replayed when it fails.
    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self) -> u64 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            self.0 >> 33
        }
        fn below(&mut self, n: usize) -> usize {
            if n == 0 { 0 } else { (self.next() as usize) % n }
        }
    }

    /// LSP position → byte offset, by the spec's own rules and nothing else.
    fn ref_pos_to_byte(text: &str, line: u32, character: u32) -> usize {
        let b = text.as_bytes();
        let mut idx = 0usize;
        let mut cur = 0u32;
        while cur < line && idx < b.len() {
            match b[idx] {
                b'\n' => { idx += 1; cur += 1; }
                b'\r' => {
                    idx += 1;
                    if idx < b.len() && b[idx] == b'\n' { idx += 1; }
                    cur += 1;
                }
                _ => idx += 1,
            }
        }
        let mut cu = 0u32;
        for ch in text[idx..].chars() {
            if ch == '\n' || ch == '\r' { break }
            if cu >= character { break }
            cu += ch.len_utf16() as u32;
            idx += ch.len_utf8();
        }
        idx
    }

    /// Byte offset → LSP position, the same rules read the other way.
    fn ref_byte_to_pos(text: &str, off: usize) -> Position {
        let b = text.as_bytes();
        let mut idx = 0usize;
        let mut line = 0u32;
        let mut line_start = 0usize;
        while idx < off && idx < b.len() {
            match b[idx] {
                b'\n' => { idx += 1; line += 1; line_start = idx; }
                b'\r' => {
                    idx += 1;
                    if idx < b.len() && b[idx] == b'\n' { idx += 1; }
                    line += 1;
                    line_start = idx;
                }
                _ => idx += 1,
            }
        }
        let off = off.min(text.len()).max(line_start);
        let character = text[line_start..off]
            .chars()
            .map(|c| c.len_utf16() as u32)
            .sum();
        Position { line, character }
    }

    /// The editor's own model of the edit: replace a byte range in a String.
    fn ref_apply(text: &mut String, change: &TextDocumentContentChangeEvent) {
        match &change.range {
            None => *text = change.text.clone(),
            Some(r) => {
                let s = ref_pos_to_byte(text, r.start.line, r.start.character);
                let e = ref_pos_to_byte(text, r.end.line, r.end.character);
                if s > e { return }
                text.replace_range(s..e, &change.text);
            }
        }
    }

    /// Snap a byte index onto a position an editor could actually address: a
    /// char boundary, and never between the `\r` and the `\n` of one CRLF --
    /// that gap is a valid char boundary but not a place a cursor can be, and
    /// generating it tests the harness rather than the code.
    fn snap(text: &str, mut i: usize) -> usize {
        if i > text.len() { i = text.len() }
        while i > 0 && !text.is_char_boundary(i) { i -= 1 }
        let b = text.as_bytes();
        if i > 0 && i < b.len() && b[i - 1] == b'\r' && b[i] == b'\n' { i -= 1 }
        i
    }

    /// One randomized run: `n` edits over `seed`, comparing after EVERY edit so
    /// the failure names the first bad step instead of the wreckage at the end.
    fn drive(start: &str, seed: u64, n: usize) {
        let inserts = [
            "", "x", "\n", "\r\n", "abc", "мир", "\n    ro x = 1\n",
            "}", "// комментарий\n", "()", "  ", "\t", "@peek()",
        ];
        let mut rope = Rope::from_str(start);
        let mut refr = start.to_string();
        let mut rng = Lcg(seed);

        for step in 0..n {
            let a = snap(&refr, rng.below(refr.len() + 1));
            let b = snap(&refr, rng.below(refr.len() + 1));
            let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
            let ch = TextDocumentContentChangeEvent {
                range: Some(Range {
                    start: ref_byte_to_pos(&refr, lo),
                    end: ref_byte_to_pos(&refr, hi),
                }),
                range_length: None,
                text: inserts[rng.below(inserts.len())].to_string(),
            };

            ref_apply(&mut refr, &ch);
            apply_changes(&mut rope, std::slice::from_ref(&ch));

            let got = rope.to_string();
            assert_eq!(
                got, refr,
                "seed {seed}, step {step}: the server's copy drifted from the editor's.\n\
                 edit was {:?} -> {:?}\n\
                 server : {:?}\n\
                 editor : {:?}",
                ch.range, ch.text, got, refr
            );
        }
    }

    #[test]
    fn prop1_lf_document_stays_in_sync_over_a_long_edit_sequence() {
        let src = "module a

fn main() -> () {
    ro x = 1
    println(\"hi\")
}
";
        for seed in 1..=24u64 {
            drive(src, seed, 60);
        }
    }

    #[test]
    fn prop2_crlf_document_stays_in_sync() {
        // The endings this project actually ships on Windows.
        let src = "module a\r\n\r\nfn main() -> () {\r\n    ro x = 1\r\n}\r\n";
        for seed in 101..=124u64 {
            drive(src, seed, 60);
        }
    }

    #[test]
    fn prop3_cyrillic_comments_do_not_break_the_utf16_walk() {
        // Non-ASCII is where a byte/char/UTF-16 mix-up shows up first, and the
        // compiler's own sources are full of Russian comments.
        let src = "module a
// комментарий раз
fn main() -> () {
    // два
    ro x = 1
}
";
        for seed in 201..=224u64 {
            drive(src, seed, 60);
        }
    }

    #[test]
    fn prop4_mixed_endings_stay_in_sync() {
        // A mixed file is bad hygiene (check-mixed-eol) but it must not make the
        // server and the editor disagree about where anything is.
        let src = "module a
fn f() -> () {
    ro x = 1
}
";
        for seed in 301..=324u64 {
            drive(src, seed, 60);
        }
    }

    /// Same invariant, but the edits arrive BATCHED, as an editor sends them.
    /// Each change in one notification is defined against the result of the
    /// previous change in that same notification -- computing them all against
    /// the pre-batch text is the classic way to get this wrong, and it is
    /// invisible to every single-edit test.
    fn drive_batched(start: &str, seed: u64, notifications: usize) {
        let inserts = [
            "", "x", "\n", "abc", "мир", "()", "}", "\n    ro y = 2\n", "  ",
        ];
        let mut rope = Rope::from_str(start);
        let mut refr = start.to_string();
        let mut rng = Lcg(seed);

        for n in 0..notifications {
            let batch_len = 1 + rng.below(4);
            let mut batch: Vec<TextDocumentContentChangeEvent> = Vec::new();

            // Build the batch against a running copy, exactly as the spec reads:
            // change k is expressed in the coordinates left by change k-1.
            let mut staged = refr.clone();
            for _ in 0..batch_len {
                let a = snap(&staged, rng.below(staged.len() + 1));
                let b = snap(&staged, rng.below(staged.len() + 1));
                let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
                let ch = TextDocumentContentChangeEvent {
                    range: Some(Range {
                        start: ref_byte_to_pos(&staged, lo),
                        end: ref_byte_to_pos(&staged, hi),
                    }),
                    range_length: None,
                    text: inserts[rng.below(inserts.len())].to_string(),
                };
                ref_apply(&mut staged, &ch);
                batch.push(ch);
            }

            for ch in &batch {
                ref_apply(&mut refr, ch);
            }
            apply_changes(&mut rope, &batch);

            let got = rope.to_string();
            assert_eq!(
                got, refr,
                "seed {seed}, notification {n} of {batch_len} change(s): the server's \
                 copy drifted from the editor's.\nserver : {:?}\neditor : {:?}",
                got, refr
            );
        }
    }

    #[test]
    fn prop6_batched_changes_in_one_notification_stay_in_sync() {
        let src = "module a\n\nfn main() -> () {\n    ro x = 1\n    println(\"hi\")\n}\n";
        for seed in 401..=424u64 {
            drive_batched(src, seed, 40);
        }
    }

    #[test]
    fn prop7_batched_changes_on_a_crlf_document_stay_in_sync() {
        let src = "module a\r\nfn main() -> () {\r\n    ro x = 1\r\n}\r\n";
        for seed in 501..=524u64 {
            drive_batched(src, seed, 40);
        }
    }

    #[test]
    fn prop5_document_can_be_emptied_and_refilled() {
        // Select-all + type-over: the range covers the whole document, including
        // the position one past the last line, which is where clamping bugs live.
        let mut rope = Rope::from_str("a
b
c
");
        let mut refr = String::from("a
b
c
");
        for (sl, sc, el, ec, txt) in [
            (0u32, 0u32, 3u32, 0u32, ""),
            (0, 0, 0, 0, "new
text
"),
            (0, 0, 2, 0, ""),
            (0, 0, 0, 0, "x"),
        ] {
            let ch = change(sl, sc, el, ec, txt);
            ref_apply(&mut refr, &ch);
            apply_changes(&mut rope, std::slice::from_ref(&ch));
            assert_eq!(rope.to_string(), refr, "edit {sl}:{sc}..{el}:{ec} -> {txt:?}");
        }
    }
}
