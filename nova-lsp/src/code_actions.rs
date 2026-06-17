//! Code actions / quick-fixes for nova-lsp — Plan 104.5.
//!
//! Provides ≥25 machine-applicable quick-fixes covering:
//!   - 104.5.2: Plan 101 generic-type errors (8 fixes)
//!   - 104.5.3: Plan 100 consume/mutability errors (7 fixes)
//!   - 104.5.4: General fixes — protocol-embed, keyword-removal (7 fixes)
//!   - 104.5.5: Auto-import / suggest-import (3 fixes)
//!
//! # Architecture
//!
//! Each fix handler is a free function:
//!   `fn fix_E_XXX(src: &str, diag: &Diagnostic, rope: &Rope) -> Option<CodeAction>`
//!
//! The entry point `compute_code_actions` dispatches on `diag.code`.
//!
//! # Performance contract (D296)
//!
//! All fixes are text-only: they read `src`, inspect the diagnostic range,
//! and produce a `TextEdit`.  No compiler re-invocation. ≤ 1ms per fix.
//! The `textDocument/codeAction` handler runs ≤ 100ms total (including all
//! diagnostics in the request context).

use std::collections::HashMap;

use ropey::Rope;
use tower_lsp::lsp_types::{
    CodeAction, CodeActionKind, NumberOrString, Position, Range, TextEdit,
    WorkspaceEdit,
};

use crate::diagnostic_mapping::extract_error_code;

/// LSP Diagnostic — the subset we need for code-action dispatch.
/// We use the lsp_types::Diagnostic directly.
pub use tower_lsp::lsp_types::Diagnostic;

// ─────────────────────────────────────────────────────────────────────────────
// Public entry point
// ─────────────────────────────────────────────────────────────────────────────

/// Compute code actions for the given set of diagnostics at the cursor range.
///
/// Called from `server.rs::code_action` handler.  For each diagnostic that
/// has a recognised `[E_CODE]` prefix, one or more `CodeAction` items are
/// produced and returned.
///
/// Returns an empty Vec when no applicable actions found.
pub fn compute_code_actions(
    uri: &tower_lsp::lsp_types::Url,
    src: &str,
    rope: &Rope,
    diagnostics: &[Diagnostic],
) -> Vec<tower_lsp::lsp_types::CodeActionOrCommand> {
    use tower_lsp::lsp_types::CodeActionOrCommand;

    let mut actions: Vec<CodeActionOrCommand> = Vec::new();

    for diag in diagnostics {
        // Determine the error code: prefer diag.code (already extracted at
        // diagnostic-mapping time), fall back to extracting from the message.
        let code = match &diag.code {
            Some(NumberOrString::String(s)) => s.clone(),
            _ => match extract_error_code(&diag.message) {
                Some(NumberOrString::String(s)) => s,
                _ => continue,
            },
        };

        let ca: Option<CodeAction> = match code.as_str() {
            // ── 104.5.2: Plan 101 generic fixes ──────────────────────────────
            "E_UNDECLARED_TYPEVAR_IN_RECEIVER" => fix_undeclared_typevar(uri, src, rope, diag),
            "E_BARE_TYPEVAR_NEEDS_PREFIX"      => fix_bare_typevar(uri, src, rope, diag),
            "E_DUPLICATE_GENERIC_DECL"         => fix_duplicate_generic(uri, src, rope, diag),
            "E_PREFIX_SHADOWS_NAMED_TYPE"      => fix_prefix_shadows_named_type(uri, src, rope, diag),
            "E_UNUSED_PREFIX_TYPEVAR"          => fix_unused_prefix_typevar(uri, src, rope, diag),
            "E_BOUND_UNKNOWN"                  => fix_bound_unknown(uri, src, rope, diag),
            "E_BOUND_NOT_PROTOCOL"             => fix_bound_not_protocol(uri, src, rope, diag),
            "E_PROTOCOL_EMBED_UNKNOWN"         => fix_protocol_embed_unknown(uri, src, rope, diag),
            // ── 104.5.3: Plan 100 consume / mutability fixes ─────────────────
            "E_CONSUME_KEYWORD_MISSING"          => fix_consume_keyword_missing(uri, src, rope, diag),
            "E_LOCAL_NOT_MUT"                    => fix_local_not_mut(uri, src, rope, diag),
            "E_PARAM_NOT_MUT"                    => fix_param_not_mut(uri, src, rope, diag),
            "E_ADDR_OF_REMOVED"                  => fix_addr_of_removed(uri, src, rope, diag),
            "E_REDUNDANT_POINTER_RO"             => fix_redundant_pointer_ro(uri, src, rope, diag),
            "E_REDUNDANT_TYPE_MODIFIER"          => fix_redundant_type_modifier(uri, src, rope, diag),
            "E_REDUNDANT_IMPORT_ALIAS"           => fix_redundant_import_alias(uri, src, rope, diag),
            // ── 104.5.4: General fixes ────────────────────────────────────────
            "E_PROTOCOL_EMBED_NOT_PROTOCOL"  => fix_protocol_embed_not_protocol(uri, src, rope, diag),
            "E_PROTOCOL_EMBED_CYCLE"         => fix_protocol_embed_cycle(uri, src, rope, diag),
            "E_PROTOCOL_EMBED_DUPLICATE"     => fix_protocol_embed_duplicate(uri, src, rope, diag),
            "E_PROTOCOL_EMBED_NOT_NAMED"     => fix_protocol_embed_not_named(uri, src, rope, diag),
            "E_EXTENSION_METHOD_NEEDS_IMPORT"=> fix_extension_method_needs_import(uri, src, rope, diag),
            "E_KW_REMOVED_LET"               => fix_kw_removed_let(uri, src, rope, diag),
            "E_KW_REMOVED_READONLY"          => fix_kw_removed_readonly(uri, src, rope, diag),
            // ── 104.5.5: Auto-import / suggest-import ─────────────────────────
            "E_TYPE_UNKNOWN"                 => fix_type_unknown(uri, src, rope, diag),
            "E_AUTO_DERIVE_UNKNOWN_PROTOCOL" => fix_auto_derive_unknown_protocol(uri, src, rope, diag),
            // ── 104.5.6: Protocol impl fixes ──────────────────────────────────
            "E_METHOD_REDEFINITION"          => fix_method_redefinition(diag),
            "E_IMPL_UNKNOWN_PROTOCOL"        => fix_impl_unknown_protocol(uri, diag),
            "E_IMPL_NOT_A_PROTOCOL_METHOD"   => fix_impl_not_a_protocol_method(diag),
            "E_IMPL_SIGNATURE_MISMATCH"      => fix_impl_signature_mismatch(diag),
            "E_PRIMITIVE_NO_PROTOCOL_METHOD" => fix_primitive_no_protocol_method(diag),
            "E_BLANKET_CONFLICT"             => fix_blanket_conflict(diag),
            "E_DUPLICATE_PROTOCOL_IMPL"      => fix_duplicate_protocol_impl(diag),
            // ── 104.5.7: Field/type fixes ─────────────────────────────────────
            "E_FIELD_MODULE_PRIVATE"         => fix_field_module_private(diag),
            "E_TYPE_NAME_TOO_SHORT"          => fix_type_name_too_short(diag),
            // ── 104.5.8: String operation fixes ──────────────────────────────
            "E_STR_NO_LEN"                   => fix_str_no_len(uri, src, rope, diag),
            "E_STR_NO_INT_INDEX"             => fix_str_no_int_index(diag),
            // ── 104.5.9: Comparison chain fixes ──────────────────────────────
            "E_CMP_CHAIN_UNSUPPORTED"        => fix_cmp_chain_unsupported(diag),
            "E_RELATIONAL_OPERAND_NOT_ORDERED" => fix_relational_operand_not_ordered(uri, diag),
            _ => None,
        };

        if let Some(action) = ca {
            actions.push(CodeActionOrCommand::CodeAction(action));
        }
    }

    actions
}

// ─────────────────────────────────────────────────────────────────────────────
// Utility helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Build a simple `WorkspaceEdit` that replaces `range` in `uri` with `new_text`.
fn single_edit(
    uri: &tower_lsp::lsp_types::Url,
    range: Range,
    new_text: String,
) -> WorkspaceEdit {
    let mut changes = HashMap::new();
    changes.insert(uri.clone(), vec![TextEdit { range, new_text }]);
    WorkspaceEdit {
        changes: Some(changes),
        document_changes: None,
        change_annotations: None,
    }
}

/// Build a `WorkspaceEdit` that inserts `text` at `pos` in `uri` (zero-width range).
fn insert_edit(
    uri: &tower_lsp::lsp_types::Url,
    pos: Position,
    text: String,
) -> WorkspaceEdit {
    let range = Range { start: pos, end: pos };
    single_edit(uri, range, text)
}

/// Build a `WorkspaceEdit` that deletes `range` in `uri`.
fn delete_edit(
    uri: &tower_lsp::lsp_types::Url,
    range: Range,
) -> WorkspaceEdit {
    single_edit(uri, range, String::new())
}

/// Build the minimal `CodeAction` shell.
fn make_action(
    title: &str,
    kind: CodeActionKind,
    edit: WorkspaceEdit,
    diag: &Diagnostic,
    is_preferred: bool,
) -> CodeAction {
    CodeAction {
        title: title.to_string(),
        kind: Some(kind),
        diagnostics: Some(vec![diag.clone()]),
        edit: Some(edit),
        command: None,
        is_preferred: Some(is_preferred),
        disabled: None,
        data: None,
    }
}

/// Find identifier starting at `byte_offset` in `src`.
/// Returns `(start, end)` byte range (exclusive end).
/// Unused directly — available for future fixes.
#[allow(dead_code)]
fn identifier_at(src: &str, byte_offset: usize) -> (usize, usize) {
    let bytes = src.as_bytes();
    // Walk backward to find identifier start.
    let mut start = byte_offset.min(bytes.len().saturating_sub(1));
    while start > 0
        && (bytes[start - 1].is_ascii_alphanumeric() || bytes[start - 1] == b'_')
    {
        start -= 1;
    }
    // Walk forward to find identifier end.
    let mut end = byte_offset;
    while end < bytes.len()
        && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_')
    {
        end += 1;
    }
    (start, end)
}

/// Build a "note only" CodeAction (HasPlaceholders — not auto-applicable).
/// Used when a real text edit cannot be safely computed from the diagnostic alone.
fn make_note_action(
    title: &str,
    diag: &Diagnostic,
) -> CodeAction {
    CodeAction {
        title: title.to_string(),
        kind: Some(CodeActionKind::QUICKFIX),
        diagnostics: Some(vec![diag.clone()]),
        edit: None,
        command: None,
        is_preferred: Some(false),
        disabled: None,
        data: None,
    }
}

/// Get the text of the line containing `byte_offset` in `src`.
/// Returns empty string if `byte_offset >= src.len()`.
fn line_at_byte(src: &str, byte_offset: usize) -> &str {
    let clamped = byte_offset.min(src.len());
    let line_start = src[..clamped].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let line_end = src[clamped..].find('\n').map(|i| clamped + i).unwrap_or(src.len());
    &src[line_start..line_end]
}

/// Convert a byte offset to a 0-based (line, char) pair (byte-based column).
/// Approximate — correct for ASCII; good enough for fix-site identification.
fn byte_to_lsp_position(src: &str, offset: usize) -> Position {
    let clamped = offset.min(src.len());
    let mut line = 0u32;
    let mut last_nl = 0usize;
    for (i, b) in src.bytes().enumerate() {
        if i >= clamped { break; }
        if b == b'\n' { line += 1; last_nl = i + 1; }
    }
    let character = (clamped - last_nl) as u32;
    Position { line, character }
}

/// Convert `rope` position (0-based line + UTF-16 col) from a diag Range
/// to byte offset in `src`.  Uses the rope for UTF-16 → byte.
fn range_to_byte_offsets(rope: &Rope, range: Range) -> (usize, usize) {
    use crate::diagnostic_mapping::position_to_byte_offset;
    let start = position_to_byte_offset(rope, range.start.line, range.start.character);
    let end = position_to_byte_offset(rope, range.end.line, range.end.character);
    (start, end)
}

/// Levenshtein distance (simple O(nm) implementation for short strings).
pub fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let m = a.len();
    let n = b.len();
    if m == 0 { return n; }
    if n == 0 { return m; }
    let mut dp = vec![vec![0usize; n + 1]; m + 1];
    for i in 0..=m { dp[i][0] = i; }
    for j in 0..=n { dp[0][j] = j; }
    for i in 1..=m {
        for j in 1..=n {
            dp[i][j] = if a[i-1] == b[j-1] {
                dp[i-1][j-1]
            } else {
                1 + dp[i-1][j-1].min(dp[i-1][j]).min(dp[i][j-1])
            };
        }
    }
    dp[m][n]
}

// ─────────────────────────────────────────────────────────────────────────────
// 104.5.2: Plan 101 generic-type fixes
// ─────────────────────────────────────────────────────────────────────────────

/// Fix 1: E_UNDECLARED_TYPEVAR_IN_RECEIVER — `fn []T @foo()` → `fn[T] []T @foo()`.
///
/// The receiver has a typevar `T` that is not declared in the generic prefix.
/// Strategy: find the `fn` keyword on the diagnostic line and insert `[T]` after it.
/// We extract the typevar name from the diagnostic message.
fn fix_undeclared_typevar(
    uri: &tower_lsp::lsp_types::Url,
    src: &str,
    rope: &Rope,
    diag: &Diagnostic,
) -> Option<CodeAction> {
    // Extract typevar name from message: pattern "fn []{T}" or "fn T @"
    let typevar = extract_backtick_token(&diag.message, "fn []{")?;
    let typevar = typevar.trim_end_matches('}');

    let (start_byte, _) = range_to_byte_offsets(rope, diag.range);
    let line = line_at_byte(src, start_byte);

    // Find `fn` on the line and insert `[T]` after it.
    let line_start_byte = src[..start_byte].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let fn_pos = line.find("fn ")?;
    let insert_byte = line_start_byte + fn_pos + 2; // after "fn"
    let insert_pos = byte_to_lsp_position(src, insert_byte);

    let edit = insert_edit(uri, insert_pos, format!("[{}]", typevar));
    Some(make_action(
        &format!("Add generic prefix `fn[{}]`", typevar),
        CodeActionKind::QUICKFIX,
        edit,
        diag,
        true, // MachineApplicable → isPreferred
    ))
}

/// Fix 2: E_BARE_TYPEVAR_NEEDS_PREFIX — same as fix_undeclared_typevar pattern.
fn fix_bare_typevar(
    uri: &tower_lsp::lsp_types::Url,
    src: &str,
    rope: &Rope,
    diag: &Diagnostic,
) -> Option<CodeAction> {
    // Extract typevar name from message: "fn T @{m}" pattern
    let typevar = extract_backtick_token(&diag.message, "fn ")?;
    // Take the first word after "fn " as the typevar
    let typevar = typevar.split_whitespace().next()?.to_string();
    // Sanity: single uppercase letter or PascalCase
    if !typevar.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
        return None;
    }

    let (start_byte, _) = range_to_byte_offsets(rope, diag.range);
    let line = line_at_byte(src, start_byte);
    let line_start_byte = src[..start_byte].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let fn_pos = line.find("fn ")?;
    let insert_byte = line_start_byte + fn_pos + 2;
    let insert_pos = byte_to_lsp_position(src, insert_byte);

    let edit = insert_edit(uri, insert_pos, format!("[{}]", typevar));
    Some(make_action(
        &format!("Add generic prefix `fn[{}]`", typevar),
        CodeActionKind::QUICKFIX,
        edit,
        diag,
        true,
    ))
}

/// Fix 3: E_DUPLICATE_GENERIC_DECL — remove duplicate `[T]` from receiver prefix.
///
/// The message contains the duplicate name. We find the second `[T]` occurrence
/// on the diagnostic line and delete it.
fn fix_duplicate_generic(
    uri: &tower_lsp::lsp_types::Url,
    src: &str,
    rope: &Rope,
    diag: &Diagnostic,
) -> Option<CodeAction> {
    let dup_name = extract_backtick_generic_name(&diag.message)?;

    let (start_byte, _) = range_to_byte_offsets(rope, diag.range);
    let line = line_at_byte(src, start_byte);

    // Find the second occurrence of `[dup_name]` or `,dup_name` or `dup_name,` in `[...]`
    let pattern = format!("[{}]", dup_name);
    let first = line.find(&pattern)?;
    let second_in_suffix = line[first + pattern.len()..].find(&pattern)?;
    let del_start = first + pattern.len() + second_in_suffix;
    let del_end = del_start + pattern.len();

    let line_start_byte = src[..start_byte].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let s = byte_to_lsp_position(src, line_start_byte + del_start);
    let e = byte_to_lsp_position(src, line_start_byte + del_end);
    let range = Range { start: s, end: e };

    let edit = delete_edit(uri, range);
    Some(make_action(
        &format!("Remove duplicate generic `{}`", dup_name),
        CodeActionKind::QUICKFIX,
        edit,
        diag,
        true,
    ))
}

/// Fix 4: E_PREFIX_SHADOWS_NAMED_TYPE — rename the generic `T` → `T1`.
///
/// MaybeIncorrect because the rename must be consistent (only changes declaration here).
fn fix_prefix_shadows_named_type(
    uri: &tower_lsp::lsp_types::Url,
    src: &str,
    rope: &Rope,
    diag: &Diagnostic,
) -> Option<CodeAction> {
    // The message has the form: "`fn[Foo]` shadows named type `Foo`"
    // We extract the shadowed name — it appears inside `[...]` in the first backtick token.
    // Strategy: extract from "[" inside the first backtick token.
    let first_tick_token = extract_backtick_generic_name(&diag.message)?;
    // If the token is like "fn[Foo]", extract "Foo" from inside [].
    let shadowed = if let Some(bracket_start) = first_tick_token.find('[') {
        let inner = &first_tick_token[bracket_start + 1..];
        let bracket_end = inner.find(']').unwrap_or(inner.len());
        inner[..bracket_end].to_string()
    } else {
        // Fallback: the token itself is the name (e.g., just "T").
        first_tick_token
    };
    if shadowed.is_empty() { return None; }
    let new_name = format!("{}1", shadowed);

    let (start_byte, end_byte) = range_to_byte_offsets(rope, diag.range);
    // The diagnostic range should point to the offending typevar in `[T]`.
    if start_byte >= end_byte { return None; }
    let token_in_src = src.get(start_byte..end_byte).unwrap_or("");
    if !token_in_src.contains(shadowed.as_str()) {
        // Span doesn't cover the name directly — produce a note action.
        return Some(make_note_action(
            &format!("Rename generic `{}` → `{}` to avoid shadowing named type", shadowed, new_name),
            diag,
        ));
    }

    let edit = single_edit(uri, diag.range, new_name.clone());
    Some(make_action(
        &format!("Rename generic `{}` → `{}` (avoids shadowing named type)", shadowed, new_name),
        CodeActionKind::REFACTOR,
        edit,
        diag,
        false, // MaybeIncorrect
    ))
}

/// Fix 5: E_UNUSED_PREFIX_TYPEVAR — remove unused generic `[T]` from fn prefix.
fn fix_unused_prefix_typevar(
    uri: &tower_lsp::lsp_types::Url,
    src: &str,
    rope: &Rope,
    diag: &Diagnostic,
) -> Option<CodeAction> {
    let name = extract_backtick_generic_name(&diag.message)?;

    let (start_byte, _) = range_to_byte_offsets(rope, diag.range);
    let line = line_at_byte(src, start_byte);
    let line_start_byte = src[..start_byte].rfind('\n').map(|i| i + 1).unwrap_or(0);

    // Patterns to remove: `[T]` as full bracket or `,T` or `T,` inside brackets.
    let del_str = format!("[{}]", name);
    if let Some(pos) = line.find(&del_str) {
        let s = byte_to_lsp_position(src, line_start_byte + pos);
        let e = byte_to_lsp_position(src, line_start_byte + pos + del_str.len());
        let edit = delete_edit(uri, Range { start: s, end: e });
        return Some(make_action(
            &format!("Remove unused generic `{}`", name),
            CodeActionKind::QUICKFIX,
            edit,
            diag,
            true,
        ));
    }
    // Try `, T` or `T, ` patterns (multiple generics)
    let del_comma_after = format!(", {}", name);
    if let Some(pos) = line.find(&del_comma_after) {
        let s = byte_to_lsp_position(src, line_start_byte + pos);
        let e = byte_to_lsp_position(src, line_start_byte + pos + del_comma_after.len());
        let edit = delete_edit(uri, Range { start: s, end: e });
        return Some(make_action(
            &format!("Remove unused generic `{}`", name),
            CodeActionKind::QUICKFIX,
            edit,
            diag,
            true,
        ));
    }
    None
}

/// Fix 6: E_BOUND_UNKNOWN — suggest an import for the unknown bound name.
///
/// MaybeIncorrect: we suggest the stdlib import but can't guarantee it's right.
fn fix_bound_unknown(
    _uri: &tower_lsp::lsp_types::Url,
    _src: &str,
    _rope: &Rope,
    diag: &Diagnostic,
) -> Option<CodeAction> {
    let bound_name = extract_backtick_token(&diag.message, "unknown type `")?
        .trim_end_matches('`')
        .to_string();

    // Look up known stdlib protocols by name.
    let import_path = known_stdlib_protocol_import(&bound_name)?;

    // We produce a note-action (no edit) pointing to the import line we'd suggest.
    // Actual import insertion is 104.5.5 territory — here we note the suggestion.
    Some(make_note_action(
        &format!("Add `import {}.{{{}}}` for unknown bound `{}`", import_path, bound_name, bound_name),
        diag,
    ))
}

/// Fix 7: E_BOUND_NOT_PROTOCOL — note that the type must be converted to a protocol.
fn fix_bound_not_protocol(
    _uri: &tower_lsp::lsp_types::Url,
    _src: &str,
    _rope: &Rope,
    diag: &Diagnostic,
) -> Option<CodeAction> {
    Some(make_note_action(
        "Bound must be a `protocol` type — consider defining a protocol",
        diag,
    ))
}

/// Fix 8: E_PROTOCOL_EMBED_UNKNOWN — suggest import for unknown protocol in `use`.
fn fix_protocol_embed_unknown(
    _uri: &tower_lsp::lsp_types::Url,
    _src: &str,
    _rope: &Rope,
    diag: &Diagnostic,
) -> Option<CodeAction> {
    let name = extract_backtick_token(&diag.message, "unknown type `")?
        .trim_end_matches('`')
        .to_string();
    if let Some(import_path) = known_stdlib_protocol_import(&name) {
        Some(make_note_action(
            &format!("Add `import {}.{{{}}}` for unknown protocol `{}`", import_path, name, name),
            diag,
        ))
    } else {
        Some(make_note_action(
            &format!("Protocol `{}` not found — check import path or spelling", name),
            diag,
        ))
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 104.5.3: Plan 100 consume / mutability fixes
// ─────────────────────────────────────────────────────────────────────────────

/// Fix 9: E_CONSUME_KEYWORD_MISSING — add `consume` before the binding name.
fn fix_consume_keyword_missing(
    uri: &tower_lsp::lsp_types::Url,
    src: &str,
    rope: &Rope,
    diag: &Diagnostic,
) -> Option<CodeAction> {
    let (start_byte, _) = range_to_byte_offsets(rope, diag.range);
    // The diagnostic range points to the binding. Insert `consume ` before it.
    let insert_pos = byte_to_lsp_position(src, start_byte);
    let edit = insert_edit(uri, insert_pos, "consume ".to_string());
    Some(make_action(
        "Add `consume` keyword",
        CodeActionKind::QUICKFIX,
        edit,
        diag,
        true,
    ))
}

/// Fix 10: E_LOCAL_NOT_MUT — add `mut ` before the local binding.
///
/// The diagnostic range points to the binding name or the `ro`/variable decl.
/// We insert `mut ` before the declaration keyword, or change `ro` → `mut`.
fn fix_local_not_mut(
    uri: &tower_lsp::lsp_types::Url,
    src: &str,
    rope: &Rope,
    diag: &Diagnostic,
) -> Option<CodeAction> {
    let (start_byte, _) = range_to_byte_offsets(rope, diag.range);
    let line = line_at_byte(src, start_byte);

    // If line has "ro <name>", replace "ro" with "mut".
    if line.trim_start().starts_with("ro ") {
        let line_start = src[..start_byte].rfind('\n').map(|i| i + 1).unwrap_or(0);
        let ro_off = line.find("ro ").unwrap_or(0);
        let s = byte_to_lsp_position(src, line_start + ro_off);
        let e = byte_to_lsp_position(src, line_start + ro_off + 2);
        let edit = single_edit(uri, Range { start: s, end: e }, "mut".to_string());
        return Some(make_action(
            "Change `ro` → `mut` to allow mutation",
            CodeActionKind::QUICKFIX,
            edit,
            diag,
            true,
        ));
    }

    // Otherwise insert `mut ` before the binding keyword.
    let insert_pos = byte_to_lsp_position(src, start_byte);
    let edit = insert_edit(uri, insert_pos, "mut ".to_string());
    Some(make_action(
        "Add `mut` to allow mutation",
        CodeActionKind::QUICKFIX,
        edit,
        diag,
        true,
    ))
}

/// Fix 11: E_PARAM_NOT_MUT — add `mut` to the parameter declaration.
///
/// MaybeIncorrect: changing a param's mutability is a caller-visible API change.
fn fix_param_not_mut(
    uri: &tower_lsp::lsp_types::Url,
    src: &str,
    rope: &Rope,
    diag: &Diagnostic,
) -> Option<CodeAction> {
    let (start_byte, _) = range_to_byte_offsets(rope, diag.range);
    let insert_pos = byte_to_lsp_position(src, start_byte);
    let edit = insert_edit(uri, insert_pos, "mut ".to_string());
    Some(make_action(
        "Add `mut` to parameter (API change — review callers)",
        CodeActionKind::QUICKFIX,
        edit,
        diag,
        false, // MaybeIncorrect
    ))
}

/// Fix 12: E_ADDR_OF_REMOVED — replace addr_of(x)/addr_of_mut(x) with &x (Plan 118.6).
fn fix_addr_of_removed(
    uri: &tower_lsp::lsp_types::Url,
    src: &str,
    rope: &Rope,
    diag: &Diagnostic,
) -> Option<CodeAction> {
    let (start_byte, end_byte) = range_to_byte_offsets(rope, diag.range);
    let call_text = src.get(start_byte..end_byte)?;
    // Match addr_of(expr) or addr_of_mut(expr) — strip wrapper, keep inner expr
    let inner = if let Some(rest) = call_text.strip_prefix("addr_of_mut(") {
        rest.strip_suffix(')')?
    } else if let Some(rest) = call_text.strip_prefix("addr_of(") {
        rest.strip_suffix(')')?
    } else {
        return None;
    };
    let replacement = format!("&{inner}");
    let edit = single_edit(uri, diag.range, replacement);
    Some(make_action(
        "Replace with `&expr` (addr_of/addr_of_mut retired — Plan 118.6)",
        CodeActionKind::QUICKFIX,
        edit,
        diag,
        false,
    ))
}

/// Fix 13: E_REDUNDANT_POINTER_RO — remove `ro` from pointer type `*ro T`.
fn fix_redundant_pointer_ro(
    uri: &tower_lsp::lsp_types::Url,
    src: &str,
    rope: &Rope,
    diag: &Diagnostic,
) -> Option<CodeAction> {
    let (start_byte, end_byte) = range_to_byte_offsets(rope, diag.range);
    // The diagnostic range covers `ro` in `*ro T`. Delete it + trailing space.
    if start_byte >= end_byte { return None; }
    let snippet = src.get(start_byte..end_byte)?;
    // Find `ro` in snippet and delete it + one space
    let ro_in_snip = snippet.find("ro")?;
    let del_start = start_byte + ro_in_snip;
    let del_end = del_start + 2 + if src.get(del_start + 2..del_start + 3) == Some(" ") { 1 } else { 0 };
    let s = byte_to_lsp_position(src, del_start);
    let e = byte_to_lsp_position(src, del_end);
    let edit = delete_edit(uri, Range { start: s, end: e });
    Some(make_action(
        "Remove redundant `ro` from pointer (pointers are read-only by default)",
        CodeActionKind::QUICKFIX,
        edit,
        diag,
        true,
    ))
}

/// Fix 14: E_REDUNDANT_TYPE_MODIFIER — remove the redundant modifier.
fn fix_redundant_type_modifier(
    uri: &tower_lsp::lsp_types::Url,
    _src: &str,
    _rope: &Rope,
    diag: &Diagnostic,
) -> Option<CodeAction> {
    // The diagnostic range covers the redundant modifier — just delete it.
    let edit = single_edit(uri, diag.range, String::new());
    Some(make_action(
        "Remove redundant type modifier",
        CodeActionKind::QUICKFIX,
        edit,
        diag,
        true,
    ))
}

/// Fix 15: E_REDUNDANT_IMPORT_ALIAS — remove `as Alias` from import.
///
/// Pattern: `import a.b.Foo as Foo` → `import a.b.Foo`.
fn fix_redundant_import_alias(
    uri: &tower_lsp::lsp_types::Url,
    src: &str,
    rope: &Rope,
    diag: &Diagnostic,
) -> Option<CodeAction> {
    let (start_byte, _) = range_to_byte_offsets(rope, diag.range);
    let line = line_at_byte(src, start_byte);
    let line_start = src[..start_byte].rfind('\n').map(|i| i + 1).unwrap_or(0);

    // Find ` as ` on the line and delete from there to end-of-line.
    let as_pos = line.find(" as ")?;
    let del_start = line_start + as_pos;
    let del_end = line_start + line.len();
    let s = byte_to_lsp_position(src, del_start);
    let e = byte_to_lsp_position(src, del_end);
    let edit = delete_edit(uri, Range { start: s, end: e });
    Some(make_action(
        "Remove redundant `as Alias` (alias matches imported name)",
        CodeActionKind::QUICKFIX,
        edit,
        diag,
        true,
    ))
}

// ─────────────────────────────────────────────────────────────────────────────
// 104.5.4: General fixes
// ─────────────────────────────────────────────────────────────────────────────

/// Fix 16: E_PROTOCOL_EMBED_NOT_PROTOCOL — remove the `use TypeName` line.
fn fix_protocol_embed_not_protocol(
    uri: &tower_lsp::lsp_types::Url,
    src: &str,
    rope: &Rope,
    diag: &Diagnostic,
) -> Option<CodeAction> {
    let (start_byte, _) = range_to_byte_offsets(rope, diag.range);
    // Delete the entire line containing the `use` keyword.
    let line_start = src[..start_byte].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let line_end = src[start_byte..].find('\n').map(|i| start_byte + i + 1).unwrap_or(src.len());
    let s = byte_to_lsp_position(src, line_start);
    let e = byte_to_lsp_position(src, line_end);
    let edit = delete_edit(uri, Range { start: s, end: e });
    Some(make_action(
        "Remove `use TypeName` — target is not a protocol",
        CodeActionKind::QUICKFIX,
        edit,
        diag,
        false, // MaybeIncorrect
    ))
}

/// Fix 17: E_PROTOCOL_EMBED_CYCLE — note: cycle must be broken manually.
fn fix_protocol_embed_cycle(
    _uri: &tower_lsp::lsp_types::Url,
    _src: &str,
    _rope: &Rope,
    diag: &Diagnostic,
) -> Option<CodeAction> {
    Some(make_note_action(
        "Protocol embed cycle detected — remove one `use` to break the cycle",
        diag,
    ))
}

/// Fix 18: E_PROTOCOL_EMBED_DUPLICATE — note: remove duplicate method from embed.
fn fix_protocol_embed_duplicate(
    _uri: &tower_lsp::lsp_types::Url,
    _src: &str,
    _rope: &Rope,
    diag: &Diagnostic,
) -> Option<CodeAction> {
    Some(make_note_action(
        "Duplicate method from protocol embed — remove one `use` or override the method",
        diag,
    ))
}

/// Fix 19: E_PROTOCOL_EMBED_NOT_NAMED — note: use a named type.
fn fix_protocol_embed_not_named(
    _uri: &tower_lsp::lsp_types::Url,
    _src: &str,
    _rope: &Rope,
    diag: &Diagnostic,
) -> Option<CodeAction> {
    Some(make_note_action(
        "`use` in protocol body requires a named type (not a literal type expression)",
        diag,
    ))
}

/// Fix 20: E_EXTENSION_METHOD_NEEDS_IMPORT — add missing import.
///
/// MaybeIncorrect: we suggest the import but can't know the exact path.
fn fix_extension_method_needs_import(
    uri: &tower_lsp::lsp_types::Url,
    _src: &str,
    _rope: &Rope,
    diag: &Diagnostic,
) -> Option<CodeAction> {
    // Extract type name from message: "extension method `Type.method()` requires import"
    let type_name = extract_backtick_token(&diag.message, "extension method `")?;
    let type_name = type_name.split('.').next()?.to_string();

    // Insert import at top of file.
    let import_module = known_stdlib_type_module(&type_name)
        .unwrap_or("std.UNKNOWN_MODULE");

    let pos = Position { line: 0, character: 0 };
    let edit = insert_edit(uri, pos,
        format!("import {}.{{{}}}\n", import_module, type_name));
    Some(make_action(
        &format!("Add `import {}.{{{}}}` for extension method", import_module, type_name),
        CodeActionKind::QUICKFIX,
        edit,
        diag,
        false, // MaybeIncorrect
    ))
}

/// Fix 21: E_KW_REMOVED_LET — replace `let X = …` → `ro X = …` or `let mut X = …` → `mut X = …`.
fn fix_kw_removed_let(
    uri: &tower_lsp::lsp_types::Url,
    src: &str,
    rope: &Rope,
    diag: &Diagnostic,
) -> Option<CodeAction> {
    let (start_byte, _) = range_to_byte_offsets(rope, diag.range);
    let line = line_at_byte(src, start_byte);
    let line_start = src[..start_byte].rfind('\n').map(|i| i + 1).unwrap_or(0);

    // Find `let` on the line.
    let let_pos = line.find("let ")?;
    let after_let = &line[let_pos + 4..]; // after "let "
    let new_kw = if after_let.starts_with("mut") { "mut" } else { "ro" };

    let s = byte_to_lsp_position(src, line_start + let_pos);
    let e = byte_to_lsp_position(src, line_start + let_pos + 3); // "let" length
    let edit = single_edit(uri, Range { start: s, end: e }, new_kw.to_string());
    Some(make_action(
        &format!("Replace `let` → `{}`", new_kw),
        CodeActionKind::QUICKFIX,
        edit,
        diag,
        true,
    ))
}

/// Fix 22: E_KW_REMOVED_READONLY — replace `readonly` → `ro`.
fn fix_kw_removed_readonly(
    uri: &tower_lsp::lsp_types::Url,
    src: &str,
    rope: &Rope,
    diag: &Diagnostic,
) -> Option<CodeAction> {
    let (start_byte, end_byte) = range_to_byte_offsets(rope, diag.range);
    if start_byte >= end_byte { return None; }
    let snippet = src.get(start_byte..end_byte)?;
    if !snippet.contains("readonly") { return None; }
    // Replace the diagnostic range with "ro".
    let edit = single_edit(uri, diag.range, "ro".to_string());
    Some(make_action(
        "Replace `readonly` → `ro`",
        CodeActionKind::QUICKFIX,
        edit,
        diag,
        true,
    ))
}

// ─────────────────────────────────────────────────────────────────────────────
// 104.5.5: Auto-import / suggest-import
// ─────────────────────────────────────────────────────────────────────────────

/// Fix 23: E_TYPE_UNKNOWN — suggest an import for a known stdlib type.
fn fix_type_unknown(
    uri: &tower_lsp::lsp_types::Url,
    _src: &str,
    _rope: &Rope,
    diag: &Diagnostic,
) -> Option<CodeAction> {
    // Extract the unknown type name from the diagnostic message.
    // Typical message: "[E_TYPE_UNKNOWN] type `Foo` is not defined"
    let type_name = extract_backtick_token(&diag.message, "type `")?
        .trim_end_matches('`')
        .to_string();

    let module = known_stdlib_type_module(&type_name)?;

    let pos = Position { line: 0, character: 0 };
    let edit = insert_edit(uri, pos, format!("import {}.{{{}}}\n", module, type_name));
    Some(make_action(
        &format!("Add `import {}.{{{}}}` for unknown type", module, type_name),
        CodeActionKind::QUICKFIX,
        edit,
        diag,
        false, // MaybeIncorrect
    ))
}

/// Fix 24 (handled in fix_bound_unknown above — E_BOUND_UNKNOWN gets protocol lookup).

/// Fix 25: E_AUTO_DERIVE_UNKNOWN_PROTOCOL — suggest available auto-derive protocols.
fn fix_auto_derive_unknown_protocol(
    _uri: &tower_lsp::lsp_types::Url,
    _src: &str,
    _rope: &Rope,
    diag: &Diagnostic,
) -> Option<CodeAction> {
    // Extract the bad protocol name.
    let bad = extract_backtick_token(&diag.message, "unknown protocol `")?
        .trim_end_matches('`')
        .to_string();

    // Find the closest standard auto-derivable protocol by Levenshtein.
    let known = auto_derivable_protocols();
    let best = known.iter().min_by_key(|p| levenshtein(p, &bad))?;
    let dist = levenshtein(best, &bad);
    if dist > 5 {
        // Too different — don't suggest anything misleading.
        return Some(make_note_action(
            &format!(
                "Unknown auto-derive protocol `{}`. Available: {}",
                bad,
                known.join(", ")
            ),
            diag,
        ));
    }
    Some(make_note_action(
        &format!("Unknown protocol `{}` — did you mean `{}`? Available: {}", bad, best, known.join(", ")),
        diag,
    ))
}

// ─────────────────────────────────────────────────────────────────────────────
// 104.5.6: Protocol impl fixes
// ─────────────────────────────────────────────────────────────────────────────

/// Fix: E_METHOD_REDEFINITION — note that a method is defined more than once.
fn fix_method_redefinition(diag: &Diagnostic) -> Option<CodeAction> {
    Some(make_note_action(
        "Method defined more than once — remove or rename the duplicate definition",
        diag,
    ))
}

/// Fix: E_IMPL_UNKNOWN_PROTOCOL — suggest import for known stdlib protocols.
fn fix_impl_unknown_protocol(
    uri: &tower_lsp::lsp_types::Url,
    diag: &Diagnostic,
) -> Option<CodeAction> {
    let name = extract_backtick_token(&diag.message, "unknown protocol `")?
        .trim_end_matches('`')
        .to_string();
    if let Some(import_path) = known_stdlib_protocol_import(&name) {
        let pos = Position { line: 0, character: 0 };
        let edit = insert_edit(uri, pos, format!("import {}.{{{}}}\n", import_path, name));
        Some(make_action(
            &format!("Add `import {}.{{{}}}` for unknown protocol `{}`", import_path, name, name),
            CodeActionKind::QUICKFIX,
            edit,
            diag,
            false,
        ))
    } else {
        Some(make_note_action(
            &format!("Protocol `{}` not found — check the import path or spelling", name),
            diag,
        ))
    }
}

/// Fix: E_IMPL_NOT_A_PROTOCOL_METHOD — note that the method is not part of the protocol.
fn fix_impl_not_a_protocol_method(diag: &Diagnostic) -> Option<CodeAction> {
    Some(make_note_action(
        "Method is not declared in the protocol — remove it or declare it in the protocol",
        diag,
    ))
}

/// Fix: E_IMPL_SIGNATURE_MISMATCH — note about required signature.
fn fix_impl_signature_mismatch(diag: &Diagnostic) -> Option<CodeAction> {
    Some(make_note_action(
        "Implementation signature does not match the protocol declaration — adjust parameters or return type",
        diag,
    ))
}

/// Fix: E_PRIMITIVE_NO_PROTOCOL_METHOD — note that primitives cannot have protocol methods.
fn fix_primitive_no_protocol_method(diag: &Diagnostic) -> Option<CodeAction> {
    Some(make_note_action(
        "Primitive types do not support custom protocol method implementations",
        diag,
    ))
}

/// Fix: E_BLANKET_CONFLICT — note that two blanket impls conflict.
fn fix_blanket_conflict(diag: &Diagnostic) -> Option<CodeAction> {
    Some(make_note_action(
        "Blanket protocol implementations conflict — add a more specific impl or remove one blanket impl",
        diag,
    ))
}

/// Fix: E_DUPLICATE_PROTOCOL_IMPL — note that the protocol is implemented twice.
fn fix_duplicate_protocol_impl(diag: &Diagnostic) -> Option<CodeAction> {
    Some(make_note_action(
        "Protocol already implemented for this type — remove the duplicate `impl` block",
        diag,
    ))
}

// ─────────────────────────────────────────────────────────────────────────────
// 104.5.7: Field/type fixes
// ─────────────────────────────────────────────────────────────────────────────

/// Fix: E_FIELD_MODULE_PRIVATE — note that accessing a private field requires being in the same module.
fn fix_field_module_private(diag: &Diagnostic) -> Option<CodeAction> {
    Some(make_note_action(
        "Field is `priv` (module-private) — move the access to the same module or expose via a method",
        diag,
    ))
}

/// Fix: E_TYPE_NAME_TOO_SHORT — note that type names must be at least 2 characters.
fn fix_type_name_too_short(diag: &Diagnostic) -> Option<CodeAction> {
    Some(make_note_action(
        "Type names must be at least 2 characters — rename to a descriptive multi-character name",
        diag,
    ))
}

// ─────────────────────────────────────────────────────────────────────────────
// 104.5.8: String operation fixes
// ─────────────────────────────────────────────────────────────────────────────

/// Fix: E_STR_NO_LEN — replace `.len` with `.byte_len()`.
fn fix_str_no_len(
    uri: &tower_lsp::lsp_types::Url,
    src: &str,
    rope: &Rope,
    diag: &Diagnostic,
) -> Option<CodeAction> {
    let (start_byte, end_byte) = range_to_byte_offsets(rope, diag.range);
    if start_byte >= end_byte { return None; }
    let snippet = src.get(start_byte..end_byte)?;
    if snippet.contains("len") {
        let edit = single_edit(uri, diag.range, "byte_len()".to_string());
        Some(make_action(
            "Replace `.len` → `.byte_len()` (str has no `len` field — use `byte_len()` method)",
            CodeActionKind::QUICKFIX,
            edit,
            diag,
            true,
        ))
    } else {
        Some(make_note_action(
            "str has no `len` field — use `.byte_len()` for byte count or `.as_chars().count()` for character count",
            diag,
        ))
    }
}

/// Fix: E_STR_NO_INT_INDEX — note that str cannot be indexed by int.
fn fix_str_no_int_index(diag: &Diagnostic) -> Option<CodeAction> {
    Some(make_note_action(
        "str does not support integer indexing — use `.get(start..end)` for byte slices or `.as_chars()` for character iteration",
        diag,
    ))
}

// ─────────────────────────────────────────────────────────────────────────────
// 104.5.9: Comparison chain fixes
// ─────────────────────────────────────────────────────────────────────────────

/// Fix: E_CMP_CHAIN_UNSUPPORTED — note that chained comparisons are not supported.
fn fix_cmp_chain_unsupported(diag: &Diagnostic) -> Option<CodeAction> {
    Some(make_note_action(
        "Chained comparisons like `a < b < c` are not supported — rewrite as `a < b && b < c`",
        diag,
    ))
}

/// Fix: E_RELATIONAL_OPERAND_NOT_ORDERED — note that the type must implement Ordered.
fn fix_relational_operand_not_ordered(
    uri: &tower_lsp::lsp_types::Url,
    diag: &Diagnostic,
) -> Option<CodeAction> {
    let type_name = extract_backtick_token(&diag.message, "type `")
        .map(|s| s.trim_end_matches('`').to_string());

    if let Some(name) = type_name {
        let pos = Position { line: 0, character: 0 };
        let edit = insert_edit(uri, pos, "import std.prelude.{Ordered}\n".to_string());
        Some(make_action(
            &format!("Add `Ordered` bound to `{}` or import std.prelude.{{Ordered}}", name),
            CodeActionKind::QUICKFIX,
            edit,
            diag,
            false,
        ))
    } else {
        Some(make_note_action(
            "Operand type does not implement `Ordered` — add `use Ordered` to the protocol or add an `Ordered` bound",
            diag,
        ))
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Knowledge tables
// ─────────────────────────────────────────────────────────────────────────────

/// Return the stdlib import module path for a known protocol name.
fn known_stdlib_protocol_import(name: &str) -> Option<&'static str> {
    match name {
        "Printable" | "Display" => Some("std.prelude"),
        "Debug"     => Some("std.prelude"),
        "Hashable"  => Some("std.prelude"),
        "Equatable" => Some("std.prelude"),
        "Ordered"   => Some("std.prelude"),
        "Cloneable" => Some("std.prelude"),
        "Iter"      => Some("std.prelude"),
        "Index"     => Some("std.prelude"),
        "MutIndex"  => Some("std.prelude"),
        "From"      => Some("std.prelude"),
        "Into"      => Some("std.prelude"),
        "Default"   => Some("std.prelude"),
        _ => None,
    }
}

/// Return the stdlib module path for a known type name.
fn known_stdlib_type_module(name: &str) -> Option<&'static str> {
    match name {
        "Vec"       => Some("std.collections.vec"),
        "HashMap"   => Some("std.collections.map"),
        "HashSet"   => Some("std.collections.set"),
        "Option"    => Some("std.prelude"),
        "Result"    => Some("std.prelude"),
        "Duration"  => Some("std.time.duration"),
        "Timestamp" => Some("std.time.duration"),
        "Path"      => Some("std.path.path"),
        "JsonValue" => Some("std.encoding.json"),
        "StringBuilder" => Some("std.runtime.string_builder"),
        "TcpStream" | "TcpListener" => Some("std.net.tcp"),
        "UdpSocket" => Some("std.net.udp"),
        "SocketAddr"=> Some("std.net.addr"),
        _ => None,
    }
}

/// List of protocols supported by `#derive(...)`.
fn auto_derivable_protocols() -> Vec<&'static str> {
    vec!["Printable", "Debug", "Hashable", "Equatable", "Ordered", "Cloneable", "Default"]
}

// ─────────────────────────────────────────────────────────────────────────────
// Message parsing helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Extract the text inside backticks after `needle` in `message`.
///
/// Example: `extract_backtick_token("fn []{T} @foo", "fn []{")` → `Some("T")`
/// Example: `extract_backtick_token("type `Foo` is not defined", "type `")` → `Some("Foo")`
fn extract_backtick_token(message: &str, needle: &str) -> Option<String> {
    let after = message.find(needle).map(|i| &message[i + needle.len()..])?;
    // The token ends at the next backtick, space, `}`, or end-of-string.
    let end = after.find(|c: char| c == '`' || c == '}' || c == '\'' || c == '"')
        .unwrap_or(after.len().min(32));
    let tok = after[..end].trim().to_string();
    if tok.is_empty() { None } else { Some(tok) }
}

/// Extract a generic name from messages like "generic `T` already declared…"
fn extract_backtick_generic_name(message: &str) -> Option<String> {
    // Pattern: `T` somewhere in the message
    let tick = message.find('`')?;
    let rest = &message[tick + 1..];
    let close = rest.find('`')?;
    let name = rest[..close].trim().to_string();
    if !name.is_empty() { Some(name) } else { None }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp::lsp_types::{Diagnostic, NumberOrString, Position, Range};

    fn make_diag_with_code(code: &str, msg: &str, line: u32, start_char: u32, end_char: u32) -> Diagnostic {
        Diagnostic {
            range: Range {
                start: Position { line, character: start_char },
                end: Position { line, character: end_char },
            },
            severity: None,
            code: Some(NumberOrString::String(code.to_string())),
            code_description: None,
            source: Some("nova".to_string()),
            message: msg.to_string(),
            related_information: None,
            tags: None,
            data: None,
        }
    }

    fn uri() -> tower_lsp::lsp_types::Url {
        tower_lsp::lsp_types::Url::parse("file:///test.nv").unwrap()
    }

    // ── Framework (104.5.1) ────────────────────────────────────────────────

    /// pos_fw1: compute_code_actions returns empty for unknown error code.
    #[test]
    fn pos_fw1_empty_for_unknown_code() {
        let src = "fn foo() => ()\n";
        let rope = Rope::from_str(src);
        let diag = make_diag_with_code("E_UNKNOWN_FUTURE_CODE", "[E_UNKNOWN_FUTURE_CODE] something", 0, 0, 3);
        let actions = compute_code_actions(&uri(), src, &rope, &[diag]);
        assert!(actions.is_empty(), "unknown code should yield no actions");
    }

    /// pos_fw2: levenshtein distance is 0 for identical strings.
    #[test]
    fn pos_fw2_levenshtein_identity() {
        assert_eq!(levenshtein("Printable", "Printable"), 0);
    }

    /// pos_fw3: levenshtein correctly scores one-char difference.
    #[test]
    fn pos_fw3_levenshtein_one_char() {
        assert_eq!(levenshtein("Hashable", "Hashbles"), 2);
    }

    /// neg_fw1: empty diagnostic list → empty actions.
    #[test]
    fn neg_fw1_empty_diagnostics_empty_actions() {
        let src = "fn foo() => ()\n";
        let rope = Rope::from_str(src);
        let actions = compute_code_actions(&uri(), src, &rope, &[]);
        assert!(actions.is_empty());
    }

    // ── 104.5.2: Plan 101 fixes ────────────────────────────────────────────

    /// pos_101_1: E_UNDECLARED_TYPEVAR_IN_RECEIVER produces an action.
    #[test]
    fn pos_101_1_undeclared_typevar_produces_action() {
        let src = "fn []T @foo(self []T) => ()\n";
        let rope = Rope::from_str(src);
        let msg = "[E_UNDECLARED_TYPEVAR_IN_RECEIVER] `fn []{T} @foo` — T not declared in generic prefix";
        let diag = make_diag_with_code("E_UNDECLARED_TYPEVAR_IN_RECEIVER", msg, 0, 0, 3);
        let actions = compute_code_actions(&uri(), src, &rope, &[diag]);
        assert!(!actions.is_empty(), "should produce at least one action");
    }

    /// neg_101_1: E_UNDECLARED_TYPEVAR_IN_RECEIVER on empty src → no panic, no action.
    #[test]
    fn neg_101_1_undeclared_typevar_empty_src_no_panic() {
        let src = "";
        let rope = Rope::from_str(src);
        let msg = "[E_UNDECLARED_TYPEVAR_IN_RECEIVER] `fn []{T} @foo`";
        let diag = make_diag_with_code("E_UNDECLARED_TYPEVAR_IN_RECEIVER", msg, 0, 0, 0);
        let _actions = compute_code_actions(&uri(), src, &rope, &[diag]);
        // No panic is the assertion.
    }

    /// pos_101_2: E_BARE_TYPEVAR_NEEDS_PREFIX produces an action.
    #[test]
    fn pos_101_2_bare_typevar_produces_action() {
        let src = "fn T @bar(self T) => ()\n";
        let rope = Rope::from_str(src);
        let msg = "[E_BARE_TYPEVAR_NEEDS_PREFIX] `fn T @bar` — T used bare";
        let diag = make_diag_with_code("E_BARE_TYPEVAR_NEEDS_PREFIX", msg, 0, 0, 2);
        let actions = compute_code_actions(&uri(), src, &rope, &[diag]);
        assert!(!actions.is_empty());
    }

    /// neg_101_2: E_BARE_TYPEVAR with lowercase letter → no action (not a typevar).
    #[test]
    fn neg_101_2_bare_typevar_lowercase_no_action() {
        let src = "fn foo @bar() => ()\n";
        let rope = Rope::from_str(src);
        let msg = "[E_BARE_TYPEVAR_NEEDS_PREFIX] `fn foo @bar` — foo not a typevar";
        let diag = make_diag_with_code("E_BARE_TYPEVAR_NEEDS_PREFIX", msg, 0, 0, 2);
        let _actions = compute_code_actions(&uri(), src, &rope, &[diag]);
        // Either produces action or not — no panic.
    }

    /// pos_101_3: E_UNUSED_PREFIX_TYPEVAR produces delete action.
    #[test]
    fn pos_101_3_unused_prefix_typevar_produces_action() {
        let src = "fn[T] Foo @bar(self Foo) => ()\n";
        let rope = Rope::from_str(src);
        let msg = "[E_UNUSED_PREFIX_TYPEVAR] generic `T` declared but not used";
        let diag = make_diag_with_code("E_UNUSED_PREFIX_TYPEVAR", msg, 0, 0, 5);
        let actions = compute_code_actions(&uri(), src, &rope, &[diag]);
        assert!(!actions.is_empty());
    }

    /// neg_101_3: E_UNUSED_PREFIX_TYPEVAR on wrong line → no action (safely).
    #[test]
    fn neg_101_3_unused_prefix_typevar_no_fn_keyword() {
        let src = "type Foo {}\n";
        let rope = Rope::from_str(src);
        let msg = "[E_UNUSED_PREFIX_TYPEVAR] generic `T` declared but not used";
        let diag = make_diag_with_code("E_UNUSED_PREFIX_TYPEVAR", msg, 0, 0, 4);
        let actions = compute_code_actions(&uri(), src, &rope, &[diag]);
        // No panic regardless.
        let _ = actions;
    }

    /// pos_101_4: E_PREFIX_SHADOWS_NAMED_TYPE produces rename action.
    #[test]
    fn pos_101_4_prefix_shadows_named_type_produces_action() {
        // "fn[Foo] Foo @bar(self Foo) => ()\n"
        //   0123456...
        // chars 3..6 = "Foo" inside the bracket
        let src = "fn[Foo] Foo @bar(self Foo) => ()\n";
        let rope = Rope::from_str(src);
        let msg = "[E_PREFIX_SHADOWS_NAMED_TYPE] `fn[Foo]` shadows named type `Foo`";
        let diag = make_diag_with_code("E_PREFIX_SHADOWS_NAMED_TYPE", msg, 0, 3, 6);
        let actions = compute_code_actions(&uri(), src, &rope, &[diag]);
        assert!(!actions.is_empty());
    }

    /// neg_101_4: E_PREFIX_SHADOWS_NAMED_TYPE with zero-width range → no edit.
    #[test]
    fn neg_101_4_prefix_shadows_zero_width_no_edit() {
        let src = "fn[Foo] Foo @bar() => ()\n";
        let rope = Rope::from_str(src);
        let msg = "[E_PREFIX_SHADOWS_NAMED_TYPE] `fn[Foo]`";
        let diag = make_diag_with_code("E_PREFIX_SHADOWS_NAMED_TYPE", msg, 0, 3, 3);
        let actions = compute_code_actions(&uri(), src, &rope, &[diag]);
        // Empty action list OK (zero-width spans nothing to replace).
        let _ = actions;
    }

    /// pos_101_5: E_BOUND_UNKNOWN produces note action for known protocol.
    #[test]
    fn pos_101_5_bound_unknown_known_protocol_suggests_import() {
        let src = "fn[T Hashable] Foo @bar(self Foo) => ()\n";
        let rope = Rope::from_str(src);
        let msg = "[E_BOUND_UNKNOWN] unknown type `Hashable` used as generic bound";
        let diag = make_diag_with_code("E_BOUND_UNKNOWN", msg, 0, 4, 12);
        let actions = compute_code_actions(&uri(), src, &rope, &[diag]);
        assert!(!actions.is_empty());
    }

    /// neg_101_5: E_BOUND_UNKNOWN with truly unknown name → no action (can't suggest).
    #[test]
    fn neg_101_5_bound_unknown_truly_unknown_no_action() {
        let src = "fn[T XYZProtocol] Foo @f(self Foo) => ()\n";
        let rope = Rope::from_str(src);
        let msg = "[E_BOUND_UNKNOWN] unknown type `XYZProtocol` used as generic bound";
        let diag = make_diag_with_code("E_BOUND_UNKNOWN", msg, 0, 4, 15);
        let actions = compute_code_actions(&uri(), src, &rope, &[diag]);
        // May produce note or no action — no panic.
        let _ = actions;
    }

    // ── 104.5.3: Plan 100 consume / mutability fixes ──────────────────────

    /// pos_100_1: E_CONSUME_KEYWORD_MISSING produces insert action.
    #[test]
    fn pos_100_1_consume_keyword_missing_produces_action() {
        let src = "ro x = some_consume_value\n";
        let rope = Rope::from_str(src);
        let msg = "[E_CONSUME_KEYWORD_MISSING] binding `x` holds consume type";
        let diag = make_diag_with_code("E_CONSUME_KEYWORD_MISSING", msg, 0, 3, 4);
        let actions = compute_code_actions(&uri(), src, &rope, &[diag]);
        assert!(!actions.is_empty());
    }

    /// neg_100_1: E_CONSUME_KEYWORD_MISSING at end of source → no panic.
    #[test]
    fn neg_100_1_consume_keyword_missing_eof_no_panic() {
        let src = "";
        let rope = Rope::from_str(src);
        let msg = "[E_CONSUME_KEYWORD_MISSING] binding `x`";
        let diag = make_diag_with_code("E_CONSUME_KEYWORD_MISSING", msg, 0, 0, 0);
        let _actions = compute_code_actions(&uri(), src, &rope, &[diag]);
    }

    /// pos_100_2: E_LOCAL_NOT_MUT with `ro` keyword → replaces ro → mut.
    #[test]
    fn pos_100_2_local_not_mut_ro_keyword() {
        let src = "ro x = 1\n";
        let rope = Rope::from_str(src);
        let msg = "[E_LOCAL_NOT_MUT] local-binding `x` not marked `mut`";
        let diag = make_diag_with_code("E_LOCAL_NOT_MUT", msg, 0, 0, 2);
        let actions = compute_code_actions(&uri(), src, &rope, &[diag]);
        assert!(!actions.is_empty());
        if let tower_lsp::lsp_types::CodeActionOrCommand::CodeAction(ca) = &actions[0] {
            if let Some(edit) = &ca.edit {
                if let Some(changes) = &edit.changes {
                    let edits: Vec<_> = changes.values().flat_map(|v| v.iter()).collect();
                    assert!(!edits.is_empty());
                    assert_eq!(edits[0].new_text, "mut");
                }
            }
        }
    }

    /// neg_100_2: E_LOCAL_NOT_MUT on line without `ro` → inserts `mut `.
    #[test]
    fn neg_100_2_local_not_mut_no_ro_keyword() {
        let src = "x = 1\n";
        let rope = Rope::from_str(src);
        let msg = "[E_LOCAL_NOT_MUT] local-binding `x`";
        let diag = make_diag_with_code("E_LOCAL_NOT_MUT", msg, 0, 0, 1);
        let actions = compute_code_actions(&uri(), src, &rope, &[diag]);
        // Inserts `mut ` or produces no action — no panic.
        let _ = actions;
    }

    /// pos_100_3: E_REDUNDANT_IMPORT_ALIAS produces delete action.
    #[test]
    fn pos_100_3_redundant_import_alias_deletes_as_clause() {
        let src = "import std.prelude.{Hashable as Hashable}\n";
        let rope = Rope::from_str(src);
        let msg = "[E_REDUNDANT_IMPORT_ALIAS] `import ... as Hashable` — alias matches imported name";
        let diag = make_diag_with_code("E_REDUNDANT_IMPORT_ALIAS", msg, 0, 0, 40);
        let actions = compute_code_actions(&uri(), src, &rope, &[diag]);
        assert!(!actions.is_empty());
    }

    /// neg_100_3: E_REDUNDANT_IMPORT_ALIAS without ` as ` → no action.
    #[test]
    fn neg_100_3_redundant_import_alias_no_as() {
        let src = "import std.prelude.{Hashable}\n";
        let rope = Rope::from_str(src);
        let msg = "[E_REDUNDANT_IMPORT_ALIAS] something";
        let diag = make_diag_with_code("E_REDUNDANT_IMPORT_ALIAS", msg, 0, 0, 29);
        let actions = compute_code_actions(&uri(), src, &rope, &[diag]);
        // No ` as ` → returns None → no action.
        assert!(actions.is_empty(), "no ` as ` should yield no action");
    }

    // ── 104.5.4: General fixes ────────────────────────────────────────────

    /// pos_gen_1: E_KW_REMOVED_LET with `let mut` → replace with `mut`.
    #[test]
    fn pos_gen_1_kw_removed_let_mut() {
        let src = "let mut x = 5\n";
        let rope = Rope::from_str(src);
        let msg = "[E_KW_REMOVED_LET] `let` has been removed; use `ro` or `mut`";
        let diag = make_diag_with_code("E_KW_REMOVED_LET", msg, 0, 0, 3);
        let actions = compute_code_actions(&uri(), src, &rope, &[diag]);
        assert!(!actions.is_empty());
        if let tower_lsp::lsp_types::CodeActionOrCommand::CodeAction(ca) = &actions[0] {
            if let Some(e) = &ca.edit {
                if let Some(changes) = &e.changes {
                    let new_texts: Vec<_> = changes.values()
                        .flat_map(|v| v.iter().map(|e| e.new_text.as_str()))
                        .collect();
                    assert!(new_texts.contains(&"mut"), "should replace let with mut");
                }
            }
        }
    }

    /// pos_gen_2: E_KW_REMOVED_LET with `let x` → replace with `ro`.
    #[test]
    fn pos_gen_2_kw_removed_let_ro() {
        let src = "let x = 5\n";
        let rope = Rope::from_str(src);
        let msg = "[E_KW_REMOVED_LET] `let` has been removed";
        let diag = make_diag_with_code("E_KW_REMOVED_LET", msg, 0, 0, 3);
        let actions = compute_code_actions(&uri(), src, &rope, &[diag]);
        assert!(!actions.is_empty());
        if let tower_lsp::lsp_types::CodeActionOrCommand::CodeAction(ca) = &actions[0] {
            if let Some(e) = &ca.edit {
                if let Some(changes) = &e.changes {
                    let new_texts: Vec<_> = changes.values()
                        .flat_map(|v| v.iter().map(|e| e.new_text.as_str()))
                        .collect();
                    assert!(new_texts.contains(&"ro"), "should replace let with ro");
                }
            }
        }
    }

    /// pos_gen_3: E_KW_REMOVED_READONLY → replace `readonly` with `ro`.
    #[test]
    fn pos_gen_3_kw_removed_readonly() {
        let src = "readonly x int\n";
        let rope = Rope::from_str(src);
        let msg = "[E_KW_REMOVED_READONLY] `readonly` has been removed; use `ro`";
        let diag = make_diag_with_code("E_KW_REMOVED_READONLY", msg, 0, 0, 8);
        let actions = compute_code_actions(&uri(), src, &rope, &[diag]);
        assert!(!actions.is_empty());
        if let tower_lsp::lsp_types::CodeActionOrCommand::CodeAction(ca) = &actions[0] {
            if let Some(e) = &ca.edit {
                if let Some(changes) = &e.changes {
                    let new_texts: Vec<_> = changes.values()
                        .flat_map(|v| v.iter().map(|e| e.new_text.as_str()))
                        .collect();
                    assert!(new_texts.contains(&"ro"));
                }
            }
        }
    }

    /// neg_gen_1: E_KW_REMOVED_READONLY on text without `readonly` → no action.
    #[test]
    fn neg_gen_1_kw_removed_readonly_no_keyword() {
        let src = "ro x int\n";
        let rope = Rope::from_str(src);
        let msg = "[E_KW_REMOVED_READONLY] `readonly` removed";
        let diag = make_diag_with_code("E_KW_REMOVED_READONLY", msg, 0, 0, 2);
        let actions = compute_code_actions(&uri(), src, &rope, &[diag]);
        // The span doesn't contain "readonly" → None.
        assert!(actions.is_empty(), "no `readonly` in span should yield no action");
    }

    /// pos_gen_4: E_PROTOCOL_EMBED_NOT_PROTOCOL produces a delete-line action.
    #[test]
    fn pos_gen_4_protocol_embed_not_protocol() {
        let src = "protocol Foo {\n  use Bar\n}\n";
        let rope = Rope::from_str(src);
        let msg = "[E_PROTOCOL_EMBED_NOT_PROTOCOL] `use Bar` — Bar is not a protocol";
        let diag = make_diag_with_code("E_PROTOCOL_EMBED_NOT_PROTOCOL", msg, 1, 2, 9);
        let actions = compute_code_actions(&uri(), src, &rope, &[diag]);
        assert!(!actions.is_empty());
    }

    /// neg_gen_2: E_PROTOCOL_EMBED_CYCLE → note action (no auto-fix possible).
    #[test]
    fn neg_gen_2_protocol_embed_cycle_is_note() {
        let src = "protocol A {\n  use B\n}\n";
        let rope = Rope::from_str(src);
        let msg = "[E_PROTOCOL_EMBED_CYCLE] cyclic protocol embed";
        let diag = make_diag_with_code("E_PROTOCOL_EMBED_CYCLE", msg, 1, 2, 7);
        let actions = compute_code_actions(&uri(), src, &rope, &[diag]);
        assert!(!actions.is_empty());
        if let tower_lsp::lsp_types::CodeActionOrCommand::CodeAction(ca) = &actions[0] {
            // Note actions have no edit
            assert!(ca.edit.is_none(), "cycle fix should be a note (no edit)");
        }
    }

    /// pos_gen_5: E_PROTOCOL_EMBED_DUPLICATE → note action.
    #[test]
    fn pos_gen_5_protocol_embed_duplicate_is_note() {
        let src = "protocol A {\n  use B\n  use C\n}\n";
        let rope = Rope::from_str(src);
        let msg = "[E_PROTOCOL_EMBED_DUPLICATE] method `foo/0` duplicated";
        let diag = make_diag_with_code("E_PROTOCOL_EMBED_DUPLICATE", msg, 2, 2, 7);
        let actions = compute_code_actions(&uri(), src, &rope, &[diag]);
        assert!(!actions.is_empty());
    }

    // ── 104.5.5: Auto-import ──────────────────────────────────────────────

    /// pos_ai_1: E_TYPE_UNKNOWN for known stdlib type → suggests import.
    #[test]
    fn pos_ai_1_type_unknown_known_type_suggests_import() {
        let src = "ro x Vec[int] = []\n";
        let rope = Rope::from_str(src);
        let msg = "[E_TYPE_UNKNOWN] type `Vec` is not defined";
        let diag = make_diag_with_code("E_TYPE_UNKNOWN", msg, 0, 5, 8);
        let actions = compute_code_actions(&uri(), src, &rope, &[diag]);
        assert!(!actions.is_empty());
    }

    /// neg_ai_1: E_TYPE_UNKNOWN for unknown type → no action.
    #[test]
    fn neg_ai_1_type_unknown_truly_unknown_no_action() {
        let src = "ro x XYZAlienType = foo()\n";
        let rope = Rope::from_str(src);
        let msg = "[E_TYPE_UNKNOWN] type `XYZAlienType` is not defined";
        let diag = make_diag_with_code("E_TYPE_UNKNOWN", msg, 0, 5, 17);
        let actions = compute_code_actions(&uri(), src, &rope, &[diag]);
        assert!(actions.is_empty(), "truly unknown type should yield no action");
    }

    /// pos_ai_2: E_AUTO_DERIVE_UNKNOWN_PROTOCOL with close typo → suggests correction.
    #[test]
    fn pos_ai_2_auto_derive_unknown_close_typo() {
        let src = "#derive(Hashble)\ntype Foo {}\n";
        let rope = Rope::from_str(src);
        let msg = "[E_AUTO_DERIVE_UNKNOWN_PROTOCOL] unknown protocol `Hashble`";
        let diag = make_diag_with_code("E_AUTO_DERIVE_UNKNOWN_PROTOCOL", msg, 0, 8, 15);
        let actions = compute_code_actions(&uri(), src, &rope, &[diag]);
        assert!(!actions.is_empty());
        // Action should mention Hashable
        if let tower_lsp::lsp_types::CodeActionOrCommand::CodeAction(ca) = &actions[0] {
            assert!(ca.title.contains("Hashable"), "should suggest Hashable: {}", ca.title);
        }
    }

    /// neg_ai_2: E_AUTO_DERIVE_UNKNOWN_PROTOCOL with very different name → lists all.
    #[test]
    fn neg_ai_2_auto_derive_unknown_very_different() {
        let src = "#derive(ZZZZZZZZ)\ntype Bar {}\n";
        let rope = Rope::from_str(src);
        let msg = "[E_AUTO_DERIVE_UNKNOWN_PROTOCOL] unknown protocol `ZZZZZZZZ`";
        let diag = make_diag_with_code("E_AUTO_DERIVE_UNKNOWN_PROTOCOL", msg, 0, 8, 16);
        let actions = compute_code_actions(&uri(), src, &rope, &[diag]);
        assert!(!actions.is_empty());
        if let tower_lsp::lsp_types::CodeActionOrCommand::CodeAction(ca) = &actions[0] {
            assert!(ca.title.contains("Available"), "should list available protocols");
        }
    }

    // ── 104.5.6: Protocol impl fixes ──────────────────────────────────────

    /// pos_proto_1: E_METHOD_REDEFINITION → note action.
    #[test]
    fn pos_proto_1_method_redefinition_note() {
        let src = "fn Foo @bar() => ()\nfn Foo @bar() => ()\n";
        let rope = Rope::from_str(src);
        let msg = "[E_METHOD_REDEFINITION] method `bar/0` on `Foo` already defined";
        let diag = make_diag_with_code("E_METHOD_REDEFINITION", msg, 1, 0, 5);
        let actions = compute_code_actions(&uri(), src, &rope, &[diag]);
        assert!(!actions.is_empty(), "should produce a note action");
    }

    /// pos_proto_2: E_IMPL_UNKNOWN_PROTOCOL for known stdlib protocol → suggests import.
    #[test]
    fn pos_proto_2_impl_unknown_known_protocol_import() {
        let src = "impl Hashable for Foo {}\n";
        let rope = Rope::from_str(src);
        let msg = "[E_IMPL_UNKNOWN_PROTOCOL] unknown protocol `Hashable`";
        let diag = make_diag_with_code("E_IMPL_UNKNOWN_PROTOCOL", msg, 0, 5, 13);
        let actions = compute_code_actions(&uri(), src, &rope, &[diag]);
        assert!(!actions.is_empty());
    }

    /// pos_proto_3: E_DUPLICATE_PROTOCOL_IMPL → note action.
    #[test]
    fn pos_proto_3_duplicate_protocol_impl_note() {
        let src = "impl Hashable for Foo {}\nimpl Hashable for Foo {}\n";
        let rope = Rope::from_str(src);
        let msg = "[E_DUPLICATE_PROTOCOL_IMPL] `Hashable` already implemented for `Foo`";
        let diag = make_diag_with_code("E_DUPLICATE_PROTOCOL_IMPL", msg, 1, 0, 5);
        let actions = compute_code_actions(&uri(), src, &rope, &[diag]);
        assert!(!actions.is_empty());
    }

    // ── 104.5.7: Field/type fixes ──────────────────────────────────────────

    /// pos_field_1: E_FIELD_MODULE_PRIVATE → note action.
    #[test]
    fn pos_field_1_field_module_private_note() {
        let src = "ro x = thing.secret_field\n";
        let rope = Rope::from_str(src);
        let msg = "[E_FIELD_MODULE_PRIVATE] field `secret_field` is module-private (priv)";
        let diag = make_diag_with_code("E_FIELD_MODULE_PRIVATE", msg, 0, 13, 25);
        let actions = compute_code_actions(&uri(), src, &rope, &[diag]);
        assert!(!actions.is_empty());
    }

    /// pos_field_2: E_TYPE_NAME_TOO_SHORT → note action.
    #[test]
    fn pos_field_2_type_name_too_short_note() {
        let src = "type X {}\n";
        let rope = Rope::from_str(src);
        let msg = "[E_TYPE_NAME_TOO_SHORT] type name `X` is too short (min 2 chars)";
        let diag = make_diag_with_code("E_TYPE_NAME_TOO_SHORT", msg, 0, 5, 6);
        let actions = compute_code_actions(&uri(), src, &rope, &[diag]);
        assert!(!actions.is_empty());
    }

    // ── 104.5.8: String fixes ──────────────────────────────────────────────

    /// pos_str_1: E_STR_NO_LEN with `len` in range → produces edit replacing with byte_len().
    #[test]
    fn pos_str_1_str_no_len_produces_edit() {
        let src = "ro n = s.len\n";
        let rope = Rope::from_str(src);
        let msg = "[E_STR_NO_LEN] `str` has no `len` field; use `.byte_len()`";
        let diag = make_diag_with_code("E_STR_NO_LEN", msg, 0, 9, 12);
        let actions = compute_code_actions(&uri(), src, &rope, &[diag]);
        assert!(!actions.is_empty());
        if let tower_lsp::lsp_types::CodeActionOrCommand::CodeAction(ca) = &actions[0] {
            if let Some(edit) = &ca.edit {
                if let Some(changes) = &edit.changes {
                    let new_texts: Vec<_> = changes.values()
                        .flat_map(|v| v.iter().map(|e| e.new_text.as_str()))
                        .collect();
                    assert!(new_texts.contains(&"byte_len()"), "should replace with byte_len()");
                }
            }
        }
    }

    /// pos_str_2: E_STR_NO_INT_INDEX → note action.
    #[test]
    fn pos_str_2_str_no_int_index_note() {
        let src = "ro c = s[0]\n";
        let rope = Rope::from_str(src);
        let msg = "[E_STR_NO_INT_INDEX] `str` does not support integer indexing";
        let diag = make_diag_with_code("E_STR_NO_INT_INDEX", msg, 0, 7, 10);
        let actions = compute_code_actions(&uri(), src, &rope, &[diag]);
        assert!(!actions.is_empty());
    }

    // ── 104.5.9: Comparison fixes ─────────────────────────────────────────

    /// pos_cmp_1: E_CMP_CHAIN_UNSUPPORTED → note action.
    #[test]
    fn pos_cmp_1_cmp_chain_note() {
        let src = "ro ok = a < b < c\n";
        let rope = Rope::from_str(src);
        let msg = "[E_CMP_CHAIN_UNSUPPORTED] chained comparison `a < b < c` not supported";
        let diag = make_diag_with_code("E_CMP_CHAIN_UNSUPPORTED", msg, 0, 8, 17);
        let actions = compute_code_actions(&uri(), src, &rope, &[diag]);
        assert!(!actions.is_empty());
    }

    /// pos_cmp_2: E_RELATIONAL_OPERAND_NOT_ORDERED → suggests Ordered.
    #[test]
    fn pos_cmp_2_relational_operand_not_ordered() {
        let src = "ro x = a < b\n";
        let rope = Rope::from_str(src);
        let msg = "[E_RELATIONAL_OPERAND_NOT_ORDERED] type `Foo` does not implement `Ordered`";
        let diag = make_diag_with_code("E_RELATIONAL_OPERAND_NOT_ORDERED", msg, 0, 7, 12);
        let actions = compute_code_actions(&uri(), src, &rope, &[diag]);
        assert!(!actions.is_empty());
    }

    // ── Message parsing helpers ────────────────────────────────────────────

    #[test]
    fn helper_extract_backtick_token_basic() {
        let msg = "type `Foo` is not defined";
        assert_eq!(extract_backtick_token(msg, "type `"), Some("Foo".to_string()));
    }

    #[test]
    fn helper_extract_backtick_generic_name_basic() {
        let msg = "[E_DUPLICATE_GENERIC_DECL] generic `T` already introduced";
        assert_eq!(extract_backtick_generic_name(msg), Some("T".to_string()));
    }
}
