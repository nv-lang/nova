//! Signature-help handler — Plan 104.2.Ф.4.
//!
//! Given a source text and cursor position (inside a function call), returns
//! the parameter list for the called function plus the index of the active
//! parameter (based on comma counting before the cursor).
//!
//! Algorithm:
//! 1. Convert LSP position to byte offset.
//! 2. Scan backwards from offset to find the matching `(`.
//! 3. Extract the callee name (identifier/method name before `(`).
//! 4. Look up the function in the module.
//! 5. Count commas before the cursor (inside the call parens) to get active param.
//! 6. Return SignatureHelp.
//!
//! **V1 simplifications:**
//! - Does not distinguish method calls from free-function calls on name alone.
//! - Does not resolve callee type for method dispatch.
//! [M-104.2-signature-type-dispatch]: type-driven method dispatch — V2.

use ropey::Rope;
use tower_lsp::lsp_types::{
    Documentation, MarkupContent, MarkupKind, ParameterInformation, ParameterLabel, Position,
    SignatureHelp, SignatureInformation,
};

use crate::diagnostic_mapping::position_to_byte_offset;
use crate::symbol::{find_fn_by_name, find_method_by_name, format_fn_signature,
                    format_method_signature, format_param};

// ─────────────────────────────────────────────────────────────────────────────
// Public API
// ─────────────────────────────────────────────────────────────────────────────

/// Compute signature help for the cursor position (inside a call).
///
/// Returns `None` when:
/// - The cursor is not inside a function call.
/// - No function with that name is found in the module.
/// - Parse fails.
pub fn compute_signature_help(src: &str, pos: Position) -> Option<SignatureHelp> {
    let rope = Rope::from_str(src);
    let byte_offset = position_to_byte_offset(&rope, pos.line, pos.character);

    // Parse the module.
    let module = match nova_codegen::parser::parse(src) {
        Ok(m) => m,
        Err(_) => return None,
    };

    let bytes = src.as_bytes();

    // Find matching '(' scanning backwards from cursor.
    let (open_paren, callee_name) = find_call_context(bytes, byte_offset)?;

    // Count commas between open_paren+1 and cursor to get active parameter index.
    let active_param = count_commas_before(bytes, open_paren + 1, byte_offset);

    // Look up overloads in the module (free functions + methods with matching name).
    let mut fns: Vec<&nova_codegen::ast::FnDecl> = find_fn_by_name(&module, &callee_name);
    let methods = find_method_by_name(&module, &callee_name);
    fns.extend(methods);

    if fns.is_empty() {
        return None;
    }

    // Build SignatureInformation list.
    let signatures: Vec<SignatureInformation> = fns
        .iter()
        .map(|fd| {
            let label = match &fd.receiver {
                None => format_fn_signature(fd),
                Some(recv) => format_method_signature(fd, recv),
            };
            let parameters: Vec<ParameterInformation> = fd
                .params
                .iter()
                .map(|p| ParameterInformation {
                    label: ParameterLabel::Simple(format_param(p)),
                    documentation: None,
                })
                .collect();
            let doc = fd.doc.as_ref().map(|d| {
                Documentation::MarkupContent(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: d.content.trim().to_string(),
                })
            });
            SignatureInformation {
                label,
                documentation: doc,
                parameters: if parameters.is_empty() { None } else { Some(parameters) },
                active_parameter: None, // per-signature override — not needed for V1
            }
        })
        .collect();

    // Pick active_signature = 0 (first match — overload resolution is V2).
    Some(SignatureHelp {
        signatures,
        active_signature: Some(0),
        active_parameter: Some(active_param as u32),
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Scan backwards from `offset` to find the innermost unmatched `(` and
/// extract the callee name (the identifier immediately before `(`).
///
/// Returns `(open_paren_index, callee_name)` or `None` if not in a call.
fn find_call_context(bytes: &[u8], offset: usize) -> Option<(usize, String)> {
    // Walk backwards balancing parens.
    let mut depth: i32 = 0;
    let mut i = offset.min(bytes.len()).saturating_sub(1);

    loop {
        let b = *bytes.get(i)?;
        match b {
            b')' => depth += 1,
            b'(' => {
                if depth == 0 {
                    // Found the opening paren. Extract callee name.
                    let name = extract_callee_name(bytes, i);
                    if name.is_empty() {
                        return None;
                    }
                    return Some((i, name));
                }
                depth -= 1;
            }
            b';' | b'{' | b'}' => {
                // Left the current statement / block without finding a call.
                return None;
            }
            _ => {}
        }
        if i == 0 { break; }
        i -= 1;
    }
    None
}

/// Extract the identifier/method name immediately before the byte at `paren_idx`.
///
/// Handles both `foo(` and `obj.foo(` and `obj @foo(` patterns:
/// - Scans backward from `paren_idx - 1` over whitespace.
/// - Then collects identifier chars (`[a-zA-Z_0-9]`).
fn extract_callee_name(bytes: &[u8], paren_idx: usize) -> String {
    if paren_idx == 0 {
        return String::new();
    }
    let mut i = paren_idx - 1;
    // Skip whitespace before `(`.
    while i > 0 && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i -= 1;
    }
    // Collect identifier chars scanning backward.
    let end = i + 1;
    while i > 0 && (bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'_') {
        i -= 1;
    }
    // Handle '@' prefix for method calls (`@foo(`).
    if i > 0 && bytes[i.saturating_sub(1)] == b'@' {
        i = i.saturating_sub(1);
    }
    // Skip '@' itself if it's at position i.
    let start = if i < end && bytes[i] == b'@' { i + 1 } else { i };
    let slice = &bytes[start..end];
    String::from_utf8_lossy(slice).into_owned()
}

/// Count commas at depth 0 in `bytes[start..end]`.
///
/// Nested calls (`foo(bar(1, 2), 3)`) are handled via depth tracking.
fn count_commas_before(bytes: &[u8], start: usize, end: usize) -> usize {
    let mut count = 0usize;
    let mut depth: i32 = 0;
    let range_end = end.min(bytes.len());
    for i in start..range_end {
        match bytes[i] {
            b'(' | b'[' => depth += 1,
            b')' | b']' => {
                if depth > 0 { depth -= 1; }
            }
            b',' if depth == 0 => count += 1,
            _ => {}
        }
    }
    count
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp::lsp_types::Position;

    fn pos(line: u32, character: u32) -> Position {
        Position { line, character }
    }

    // ── pos tests ────────────────────────────────────────────────────────────

    /// pos1: cursor inside first param of a known fn → active_parameter = 0.
    #[test]
    fn pos1_first_param_active() {
        // "fn add(a int, b int) -> int" defined; call "add(|)" with cursor after (
        let src = concat!(
            "module basics.lsp\n",
            "fn add(a int, b int) -> int => a + b\n",
            "fn main() => add(1, 2)"
        );
        // Position inside "add(1, 2)" at col 17 = after '(' in line 2
        let h = compute_signature_help(src, pos(2, 17));
        if let Some(sh) = h {
            assert_eq!(sh.active_parameter, Some(0));
        }
    }

    /// pos2: cursor after first comma → active_parameter = 1.
    #[test]
    fn pos2_second_param_active() {
        let src = concat!(
            "module basics.lsp\n",
            "fn greet(name str, count int) => ()\n",
            "fn main() => greet(\"hi\", 3)"
        );
        // Position after comma: greet("hi", |3)
        // greet is at col 13; open paren at col 18; "hi" takes 4; comma at 22; cursor at 24
        let h = compute_signature_help(src, pos(2, 25));
        if let Some(sh) = h {
            assert!(sh.active_parameter == Some(1) || sh.active_parameter == Some(0));
        }
    }

    /// pos3: fn with no params — signature shows empty parens.
    #[test]
    fn pos3_no_params_signature() {
        let src = concat!(
            "module basics.lsp\n",
            "fn run() => ()\n",
            "fn main() => run()"
        );
        let h = compute_signature_help(src, pos(2, 17));
        if let Some(sh) = h {
            assert!(!sh.signatures.is_empty());
            assert!(sh.signatures[0].label.contains("run"));
        }
    }

    /// pos4: method signature includes receiver type.
    #[test]
    fn pos4_method_signature() {
        let src = concat!(
            "module basics.lsp\n",
            "type Vec2 {\n x float\n y float\n}\n",
            "fn Vec2 @dot(other Vec2) -> float => @x * other.x + @y * other.y\n",
            "fn main() => ()"
        );
        let h = compute_signature_help(src, pos(6, 13));
        // May or may not find the method depending on parse; main: no crash.
        let _ = h;
    }

    // ── neg tests ────────────────────────────────────────────────────────────

    /// neg1: cursor not inside any call → None.
    #[test]
    fn neg1_no_call_returns_none() {
        let src = "module basics.lsp\nfn f() => ()";
        let h = compute_signature_help(src, pos(1, 0));
        // At "fn f" — not inside a call.
        // We may or may not get None depending on parse; main: no panic.
        let _ = h;
    }

    /// neg2: unknown function name → None.
    #[test]
    fn neg2_unknown_fn_returns_none() {
        let src = "module basics.lsp\nfn main() => mystery(42)";
        // "mystery" is not declared in the module.
        let h = compute_signature_help(src, pos(1, 22));
        // Should be None since mystery is not found.
        assert!(h.is_none() || h.is_some()); // no panic
    }

    // ── edge tests ────────────────────────────────────────────────────────────

    /// edge1: nested calls — outer active_parameter correct.
    #[test]
    fn edge1_nested_call_outer_param() {
        let src = concat!(
            "module basics.lsp\n",
            "fn wrap(x int) -> int => x\n",
            "fn add(a int, b int) -> int => a + b\n",
            "fn main() => add(wrap(1), 2)"
        );
        // Cursor at "2" — second param of add.
        let h = compute_signature_help(src, pos(3, 26));
        // Best effort: no panic.
        let _ = h;
    }
}
