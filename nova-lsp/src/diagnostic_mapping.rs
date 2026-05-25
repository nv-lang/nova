//! Mapping nova_codegen diagnostics → lsp_types::Diagnostic.
//!
//! Plan 104.1.Ф.2: full implementation.
//! UTF-16 position conversion via ropey.

use nova_codegen::diag::Diagnostic as NovaDiag;
use ropey::Rope;
use tower_lsp::lsp_types::{
    Diagnostic, DiagnosticRelatedInformation, DiagnosticSeverity, Location,
    Position, Range, Url,
};

// ─────────────────────────────────────────────────────────────────────────────
// Public API
// ─────────────────────────────────────────────────────────────────────────────

/// Convert a Nova compiler diagnostic to an LSP `Diagnostic`.
///
/// `rope` is the rope for the file that produced the diagnostic.
/// `file_uri` is the URI of that file (used for related_information locations).
///
/// # UTF-16 positions
///
/// LSP requires UTF-16 column numbers.  We convert byte offsets to UTF-16
/// character units via `ropey`'s `char_to_utf16_cu` iterator.
/// Surrogate pairs (emoji, characters U+10000+) count as 2 UTF-16 units.
pub fn to_lsp(d: &NovaDiag, rope: &Rope, file_uri: &Url) -> Diagnostic {
    let range = span_to_range(rope, d.span.start, d.span.end);

    // Related information from notes that have a span.
    let related: Vec<DiagnosticRelatedInformation> = d
        .notes
        .iter()
        .filter_map(|note| {
            note.span.map(|s| DiagnosticRelatedInformation {
                location: Location {
                    uri: file_uri.clone(),
                    range: span_to_range(rope, s.start, s.end),
                },
                message: note.message.clone(),
            })
        })
        .collect();

    let related_information = if related.is_empty() { None } else { Some(related) };

    Diagnostic {
        range,
        severity: Some(DiagnosticSeverity::ERROR),
        code: None,
        code_description: None,
        source: Some("nova".to_string()),
        message: d.message.clone(),
        related_information,
        tags: None,
        data: None,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Internal helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Convert a byte span (start, end) to an LSP `Range` with UTF-16 positions.
///
/// - If `start > end`: log a warning and return a zero-width point range.
/// - If offsets are out of bounds: clamp to end-of-file without panicking.
pub fn span_to_range(rope: &Rope, start: usize, end: usize) -> Range {
    if start > end {
        tracing::warn!(start, end, "diagnostic span has start > end; using point range");
        let pos = byte_offset_to_position(rope, start);
        return Range { start: pos, end: pos };
    }

    Range {
        start: byte_offset_to_position(rope, start),
        end: byte_offset_to_position(rope, end),
    }
}

/// Convert a byte offset in the rope to a UTF-16 LSP `Position`.
///
/// Clamps to end-of-file if the offset exceeds the rope length.
pub fn byte_offset_to_position(rope: &Rope, byte_offset: usize) -> Position {
    // Clamp to valid range.
    let byte_offset = byte_offset.min(rope.len_bytes());

    // Convert byte offset → char index.
    let char_idx = rope.byte_to_char(byte_offset);

    // Determine which line this char is on (0-based).
    let line = rope.char_to_line(char_idx);

    // Char index of the start of this line.
    let line_start_char = rope.line_to_char(line);

    // Number of UTF-16 code units from line start to char_idx.
    // We walk the chars in [line_start_char, char_idx) and sum their
    // UTF-16 widths (1 for BMP, 2 for supplementary plane).
    let utf16_col = chars_to_utf16_cu(rope, line_start_char, char_idx);

    Position {
        line: line as u32,
        character: utf16_col as u32,
    }
}

/// Count UTF-16 code units for rope chars in `[start_char, end_char)`.
fn chars_to_utf16_cu(rope: &Rope, start_char: usize, end_char: usize) -> usize {
    let mut cu = 0usize;
    // Iterate over chunks for efficiency (avoids per-char rope traversal).
    let mut remaining = end_char - start_char;
    // Use rope slicing for the range and iterate char by char.
    if remaining == 0 {
        return 0;
    }
    let slice = rope.slice(start_char..end_char);
    for ch in slice.chars() {
        if remaining == 0 {
            break;
        }
        cu += ch.len_utf16();
        remaining = remaining.saturating_sub(1);
    }
    cu
}

/// Convert a UTF-16 line+character position to a byte offset in the rope.
///
/// Used by incremental sync (Ф.4) to apply LSP range edits.
pub fn position_to_byte_offset(rope: &Rope, line: u32, character: u32) -> usize {
    let line = line as usize;
    let character = character as usize;

    // Clamp line to valid range.
    let line = line.min(rope.len_lines().saturating_sub(1));
    let line_start_char = rope.line_to_char(line);
    let line_end_char = if line + 1 < rope.len_lines() {
        rope.line_to_char(line + 1)
    } else {
        rope.len_chars()
    };

    // Walk UTF-16 code units forward from line start.
    let mut remaining_cu = character;
    let mut char_idx = line_start_char;
    for ch in rope.slice(line_start_char..line_end_char).chars() {
        if remaining_cu == 0 {
            break;
        }
        let w = ch.len_utf16();
        if remaining_cu < w {
            // Offset is in the middle of a surrogate pair — clamp to char start.
            break;
        }
        remaining_cu -= w;
        char_idx += 1;
    }

    // Clamp char_idx to line end (handles character > line length).
    let char_idx = char_idx.min(line_end_char);
    rope.char_to_byte(char_idx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nova_codegen::diag::{Note, Span};
    use ropey::Rope;

    fn make_url(p: &str) -> Url {
        Url::parse(&format!("file:///{p}")).unwrap()
    }

    fn make_diag(msg: &str, start: usize, end: usize) -> NovaDiag {
        NovaDiag::new(msg, Span::new(start, end))
    }

    // ── pos1 ─────────────────────────────────────────────────────────────────

    #[test]
    fn pos1_simple_error_all_fields() {
        let src = "fn foo() => ()\n";
        let rope = Rope::from_str(src);
        let d = make_diag("unexpected token", 3, 6);
        let uri = make_url("foo.nv");
        let lsp = to_lsp(&d, &rope, &uri);

        assert_eq!(lsp.source.as_deref(), Some("nova"));
        assert_eq!(lsp.severity, Some(DiagnosticSeverity::ERROR));
        assert_eq!(lsp.message, "unexpected token");
        // range should be non-null
        assert!(lsp.range.start.line == 0);
    }

    // ── pos2 ─────────────────────────────────────────────────────────────────

    #[test]
    fn pos2_ascii_span_byte_equals_utf16() {
        // Pure ASCII: byte offset == UTF-16 offset
        let src = "abcdefgh\n";
        let rope = Rope::from_str(src);
        let d = make_diag("err", 2, 5);
        let uri = make_url("a.nv");
        let lsp = to_lsp(&d, &rope, &uri);

        assert_eq!(lsp.range.start, Position { line: 0, character: 2 });
        assert_eq!(lsp.range.end, Position { line: 0, character: 5 });
    }

    // ── pos3 ─────────────────────────────────────────────────────────────────

    #[test]
    fn pos3_cyrillic_utf8_correct_utf16_offset() {
        // "Привет" — each Cyrillic char is 2 bytes in UTF-8 but 1 UTF-16 unit.
        // So byte offset 4 = char index 2 (П=0,р=1,и=2 → byte 4).
        let src = "Привет\n";
        let rope = Rope::from_str(src);
        // byte 0..12 spans "Привет" (6 chars × 2 bytes)
        let d = make_diag("err", 0, 12);
        let uri = make_url("b.nv");
        let lsp = to_lsp(&d, &rope, &uri);

        // start: char 0 → UTF-16 col 0
        // end:   char 6 → UTF-16 col 6 (all BMP)
        assert_eq!(lsp.range.start, Position { line: 0, character: 0 });
        assert_eq!(lsp.range.end, Position { line: 0, character: 6 });
    }

    // ── pos4 ─────────────────────────────────────────────────────────────────

    #[test]
    fn pos4_emoji_surrogate_pair_utf16_len_2() {
        // "👋" is U+1F44B (above BMP) → 4 bytes UTF-8, 2 UTF-16 code units.
        let src = "hi👋ok\n";
        // h=0,i=1,👋=2..5(4bytes),o=6,k=7
        let rope = Rope::from_str(src);
        // span covering the emoji: byte 2..6
        let d = make_diag("err", 2, 6);
        let uri = make_url("c.nv");
        let lsp = to_lsp(&d, &rope, &uri);

        // start: 2 ASCII chars before → UTF-16 col 2
        // end: 2 ASCII + 1 emoji (2 CU) → UTF-16 col 4
        assert_eq!(lsp.range.start, Position { line: 0, character: 2 });
        assert_eq!(lsp.range.end, Position { line: 0, character: 4 });
    }

    // ── pos5 ─────────────────────────────────────────────────────────────────

    #[test]
    fn pos5_severity_is_always_error() {
        // Nova compiler only emits errors (no severity field in Diagnostic).
        let src = "x\n";
        let rope = Rope::from_str(src);
        let d = make_diag("something wrong", 0, 1);
        let uri = make_url("d.nv");
        let lsp = to_lsp(&d, &rope, &uri);
        assert_eq!(lsp.severity, Some(DiagnosticSeverity::ERROR));
    }

    // ── pos6 ─────────────────────────────────────────────────────────────────

    #[test]
    fn pos6_notes_with_span_become_related_information() {
        let src = "abc\ndef\n";
        let rope = Rope::from_str(src);
        let uri = make_url("e.nv");

        let mut d = make_diag("type mismatch", 0, 3);
        d.notes.push(Note {
            message: "declared here".to_string(),
            span: Some(Span::new(4, 7)),
        });
        let lsp = to_lsp(&d, &rope, &uri);

        let related = lsp.related_information.expect("should have related_information");
        assert_eq!(related.len(), 1);
        assert_eq!(related[0].message, "declared here");
        assert_eq!(related[0].location.range.start.line, 1); // "def" is on line 1
    }

    // ── neg1 ─────────────────────────────────────────────────────────────────

    #[test]
    fn neg1_start_greater_than_end_returns_point_range() {
        let src = "hello\n";
        let rope = Rope::from_str(src);
        let d = make_diag("err", 5, 2); // start > end
        let uri = make_url("f.nv");
        let lsp = to_lsp(&d, &rope, &uri);

        // Should be a point range (start == end), no panic
        assert_eq!(lsp.range.start, lsp.range.end);
    }

    // ── neg2 ─────────────────────────────────────────────────────────────────

    #[test]
    fn neg2_out_of_bounds_span_clamped_no_panic() {
        let src = "abc\n";
        let rope = Rope::from_str(src);
        let d = make_diag("err", 100, 200); // way out of bounds
        let uri = make_url("g.nv");
        // Should not panic; returns some range at end of file
        let lsp = to_lsp(&d, &rope, &uri);
        assert!(lsp.range.start.line == 0 || lsp.range.start.line == 1);
    }

    // ── edge1 ────────────────────────────────────────────────────────────────

    #[test]
    fn edge1_empty_file_span_at_zero() {
        let rope = Rope::from_str("");
        let d = make_diag("err", 0, 0);
        let uri = make_url("h.nv");
        let lsp = to_lsp(&d, &rope, &uri);
        assert_eq!(lsp.range.start, Position { line: 0, character: 0 });
        assert_eq!(lsp.range.end, Position { line: 0, character: 0 });
    }

    // ── edge2 ────────────────────────────────────────────────────────────────

    #[test]
    fn edge2_span_at_end_of_file_after_newline() {
        // file is "abc\n" (4 bytes); span at offset 4 = after the newline
        let src = "abc\n";
        let rope = Rope::from_str(src);
        let d = make_diag("err", 4, 4);
        let uri = make_url("i.nv");
        let lsp = to_lsp(&d, &rope, &uri);
        // offset 4 is at the start of line 1 (char index 4 = line 1 start)
        assert!(lsp.range.start.line <= 1);
    }

    // ── position_to_byte_offset tests ────────────────────────────────────────

    #[test]
    fn p2b_ascii_round_trips() {
        let src = "hello world\n";
        let rope = Rope::from_str(src);
        // Position (0, 6) → byte 6 ("world" starts there)
        let b = position_to_byte_offset(&rope, 0, 6);
        assert_eq!(b, 6, "ASCII position round-trip failed");
    }

    #[test]
    fn p2b_cyrillic_utf16_to_byte() {
        // "Привет\n" — 6 Cyrillic chars (each 2 bytes UTF-8, 1 UTF-16 CU)
        let src = "Привет\n";
        let rope = Rope::from_str(src);
        // UTF-16 col 3 → char index 3 → byte 6
        let b = position_to_byte_offset(&rope, 0, 3);
        assert_eq!(b, 6, "Cyrillic byte offset mismatch");
    }

    #[test]
    fn p2b_emoji_utf16_2cu_to_byte() {
        // "ab👋cd\n" — emoji at UTF-16 col 2, takes 2 CU
        // UTF-16 col 4 → after emoji → char index 3 → byte 2+4=6
        let src = "ab👋cd\n";
        let rope = Rope::from_str(src);
        let b = position_to_byte_offset(&rope, 0, 4); // col 4 = after emoji
        assert_eq!(b, 6, "emoji byte offset after surrogate pair mismatch");
    }
}
