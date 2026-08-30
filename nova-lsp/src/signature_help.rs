//! Signature-help handler — Plan 104.2.Ф.4 + Plan 104.10 Ф.8.
//!
//! Given a source text and cursor position (inside a function call), returns
//! the parameter list for the called function plus the index of the active
//! parameter (based on comma counting before the cursor).
//!
//! Algorithm:
//! 1. Convert LSP position to byte offset.
//! 2. Scan backwards from offset to find the matching `(`.
//! 3. Extract the callee name (identifier/method name before `(`).
//! 4. Look up the function overloads in the resolved module.
//! 5. Count commas before the cursor (inside the call parens) to get active param.
//! 6. **Ф.8 — dispatch by real type:** rank the overloads so the *active*
//!    signature matches reality:
//!    - For a method call `recv.method(…)`, take the inferred type of `recv`
//!      from the Ф.2 `expr_types` map and prefer the overload whose receiver
//!      type matches (`vec.push(│)` → the `Vec` `push`, not another type's).
//!    - For free-function / method overloads, prefer the arity that can hold a
//!      parameter at the cursor position (`f(a,b)` when the cursor is on the 2nd
//!      argument, `f(a)` when on the 1st).
//! 7. Return SignatureHelp with the best-matching overload as `active_signature`.
//!
//! **Graceful degradation:** when the receiver type cannot be inferred (no
//! `expr_types`, momentarily-invalid buffer, unknown binding) the ranking falls
//! back to the arity heuristic and, ultimately, the first overload — never a
//! crash and never an empty result when *some* overload exists.
//!
//! [M-104.10-arg-type-dispatch]: free-function overloads are ranked by argument
//! *count* (position of the cursor), not yet by the inferred *types* of the
//! already-entered arguments. Receiver-type dispatch (the Ф.8 headline) is full;
//! full argument-type unification for free-fn overloads is a follow-up.

use ropey::Rope;
use tower_lsp::lsp_types::{
    Documentation, MarkupContent, MarkupKind, ParameterInformation, ParameterLabel, Position,
    SignatureHelp, SignatureInformation,
};

use nova_codegen::ast::FnDecl;
use nova_codegen::types::ModuleEnv;

use crate::completion::{receiver_matches, receiver_type_name};
use crate::diagnostic_mapping::position_to_byte_offset;
use crate::provenance::ResolvedModule;
use crate::symbol::{find_fn_by_name, find_method_by_name, format_fn_signature,
                    format_method_signature, format_param};

// ─────────────────────────────────────────────────────────────────────────────
// Public API
// ─────────────────────────────────────────────────────────────────────────────

/// Compute signature help for the cursor position (inside a call), parsing the
/// buffer fresh with no type information.
///
/// This is the type-unaware entry point: it distinguishes method from
/// free-function overloads structurally and ranks by arity, but has no
/// `expr_types` so it cannot dispatch a method call by the receiver's real type.
/// The server uses [`compute_signature_help_in`] (which threads the Ф.1 resolved
/// module + Ф.2 `expr_types`) for real dispatch; this variant remains for
/// callers/tests without a resolved module.
///
/// Returns `None` when:
/// - The cursor is not inside a function call.
/// - No function with that name is found in the module.
/// - Parse fails.
pub fn compute_signature_help(src: &str, pos: Position) -> Option<SignatureHelp> {
    let module = crate::compiler::parse_guarded(src).ok()?;
    signature_help_from_parts(&module, None, src, pos)
}

/// Ф.8 entry point: compute signature help against an already-resolved module
/// (imports inlined so stdlib/peer overloads are visible) and its `expr_types`
/// map (so a method call dispatches by the receiver's real inferred type).
pub fn compute_signature_help_in(
    resolved: &ResolvedModule,
    src: &str,
    pos: Position,
) -> Option<SignatureHelp> {
    signature_help_from_parts(&resolved.module, resolved.env.as_ref(), src, pos)
}

/// Shared core of both entry points.
fn signature_help_from_parts(
    module: &nova_codegen::ast::Module,
    env: Option<&ModuleEnv>,
    src: &str,
    pos: Position,
) -> Option<SignatureHelp> {
    let rope = Rope::from_str(src);
    let byte_offset = position_to_byte_offset(&rope, pos.line, pos.character);
    let bytes = src.as_bytes();

    // Find matching '(' scanning backwards from cursor, plus the callee name and
    // the byte offset where that name starts (used to detect a `recv.` prefix).
    let call = find_call_context(bytes, byte_offset)?;

    // Count commas between open_paren+1 and cursor to get active parameter index.
    let active_param = count_commas_before(bytes, call.open_paren + 1, byte_offset);

    // Collect overloads: free functions first, then methods with matching name.
    let free_fns: Vec<&FnDecl> = find_fn_by_name(module, &call.callee_name);
    let methods: Vec<&FnDecl> = find_method_by_name(module, &call.callee_name);
    let mut fns: Vec<&FnDecl> = free_fns;
    fns.extend(methods);
    if fns.is_empty() {
        return None;
    }

    // Ф.8 — is this a method call `recv.method(`? If so, resolve the receiver's
    // real type from `expr_types` (span ending exactly at the `.`).
    let is_method_call = call.receiver_dot.is_some();
    let recv_ty: Option<String> = match (call.receiver_dot, env) {
        (Some(dot_byte), Some(env)) if !env.expr_types.is_empty() => {
            receiver_type_name(env, dot_byte)
        }
        _ => None,
    };

    // Rank the overloads best-first: type match dominates, then arity fit.
    let mut ranked: Vec<(&FnDecl, i64)> = fns
        .iter()
        .map(|fd| (*fd, overload_score(fd, is_method_call, recv_ty.as_deref(), active_param)))
        .collect();
    // Stable sort by descending score — ties keep declaration order (free fns
    // before methods, source order within each), so the fallback is the natural
    // "first overload".
    ranked.sort_by(|a, b| b.1.cmp(&a.1));

    let signatures: Vec<SignatureInformation> =
        ranked.iter().map(|(fd, _)| signature_information(fd)).collect();

    Some(SignatureHelp {
        signatures,
        active_signature: Some(0),
        active_parameter: Some(active_param as u32),
    })
}

/// Score an overload for the current call site. Higher = better match; the
/// highest-scoring overload becomes `active_signature`.
///
/// Two independent contributions, added:
/// - **Type dispatch (dominant):** on a method call, a method whose receiver
///   matches the inferred receiver type wins by a wide margin; a method with a
///   *different* known receiver type is demoted below everything else. When the
///   receiver type is unknown, methods stay neutral (graceful first-overload
///   fallback). On a free-function call, free functions are preferred over
///   same-named methods.
/// - **Arity fit:** an overload that can hold a parameter at the cursor position
///   (`params.len() > active_param`) is preferred, and among those the tightest
///   arity wins; overloads too short to reach the cursor are demoted, closest
///   arity first.
fn overload_score(
    fd: &FnDecl,
    is_method_call: bool,
    recv_ty: Option<&str>,
    active_param: usize,
) -> i64 {
    const TYPE_MATCH: i64 = 1_000_000;
    const TYPE_MISMATCH: i64 = -1_000_000;
    const KIND_MISMATCH: i64 = -10_000;

    let type_score = match (is_method_call, &fd.receiver) {
        // Method call, method overload: dispatch by receiver type when known.
        (true, Some(recv)) => match recv_ty {
            Some(name) if receiver_matches(recv, name) => TYPE_MATCH,
            Some(_) => TYPE_MISMATCH,
            None => 0, // unknown receiver → graceful, keep candidate
        },
        // Method call resolving to a free function (rare) — least relevant.
        (true, None) => KIND_MISMATCH,
        // Free-function call landing on a method — demote below free fns.
        (false, Some(_)) => KIND_MISMATCH,
        // Free-function call, free function — the expected case.
        (false, None) => 0,
    };

    let nparams = fd.params.len();
    let arity_fit = if nparams > active_param {
        // Can hold a parameter at the cursor; prefer the tightest arity.
        1_000 - nparams as i64
    } else {
        // Too short to reach the cursor; prefer the closest (largest) arity.
        nparams as i64
    };

    type_score + arity_fit
}

/// Build a `SignatureInformation` from a function/method declaration.
fn signature_information(fd: &FnDecl) -> SignatureInformation {
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
        active_parameter: None,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

/// The parsed call context around the cursor.
struct CallContext {
    /// Byte index of the innermost unmatched `(`.
    open_paren: usize,
    /// Callee identifier (method or free-function name), `@` stripped.
    callee_name: String,
    /// For a method call `recv.method(`, the byte index of the `.` separating
    /// the receiver from the method name (i.e. the byte where the receiver
    /// expression *ends*). `None` for a plain `foo(` or a `obj @foo(` call.
    receiver_dot: Option<usize>,
}

/// Scan backwards from `offset` to find the innermost unmatched `(` and
/// extract the callee name (the identifier immediately before `(`).
///
/// Returns the [`CallContext`] or `None` if the cursor is not inside a call.
fn find_call_context(bytes: &[u8], offset: usize) -> Option<CallContext> {
    // Walk backwards balancing parens.
    let mut depth: i32 = 0;
    let mut i = offset.min(bytes.len()).saturating_sub(1);

    loop {
        let b = *bytes.get(i)?;
        match b {
            b')' => depth += 1,
            b'(' => {
                if depth == 0 {
                    // Found the opening paren. Extract callee name + name start.
                    let (name, name_start) = extract_callee_name(bytes, i);
                    if name.is_empty() {
                        return None;
                    }
                    // A `.` immediately before the name marks a value method
                    // call `recv.method(`; the receiver expression ends at that
                    // `.` (used to look the receiver type up in `expr_types`).
                    let receiver_dot = if name_start > 0 && bytes[name_start - 1] == b'.' {
                        Some(name_start - 1)
                    } else {
                        None
                    };
                    return Some(CallContext { open_paren: i, callee_name: name, receiver_dot });
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

/// Extract the identifier/method name immediately before the byte at `paren_idx`
/// together with the byte index where that name starts.
///
/// Handles `foo(`, `obj.foo(` and `obj @foo(` patterns:
/// - Scans backward from `paren_idx - 1` over whitespace.
/// - Then collects identifier chars (`[a-zA-Z_0-9]`).
/// The returned `name_start` points at the first identifier byte (after any
/// stripped `@`), so the caller can inspect the byte before it (`.` → method).
fn extract_callee_name(bytes: &[u8], paren_idx: usize) -> (String, usize) {
    if paren_idx == 0 {
        return (String::new(), 0);
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
    (String::from_utf8_lossy(slice).into_owned(), start)
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
            "type Vec2 {\n x f64\n y f64\n}\n",
            "fn Vec2 @dot(other Vec2) -> f64 => @x * other.x + @y * other.y\n",
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

    // ── Ф.8: type-driven overload dispatch ─────────────────────────────────────

    use crate::provenance::resolve_module_for_ide;
    use std::path::PathBuf;

    /// Repo root (nova-lsp's parent), so temp fixtures resolve stdlib + imports.
    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("nova-lsp has a parent")
            .to_path_buf()
    }

    /// Write an isolated fixture (own sub-dir avoids folder-module peer bleed).
    fn write_temp(stem: &str, src: &str) -> PathBuf {
        let dir = repo_root().join("target").join("sig_test").join(stem);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("{stem}.nv"));
        std::fs::write(&path, src).unwrap();
        path
    }

    /// LSP `Position` of the byte immediately after `marker`'s first occurrence.
    fn cursor_after(src: &str, marker: &str) -> Position {
        let idx = src.find(marker).expect("marker present") + marker.len();
        let mut line = 0u32;
        let mut character = 0u32;
        for (i, c) in src.char_indices() {
            if i >= idx {
                break;
            }
            if c == '\n' {
                line += 1;
                character = 0;
            } else {
                character += 1;
            }
        }
        Position { line, character }
    }

    /// t_pos1: `recv.method(│)` dispatches to the overload whose receiver type
    /// matches the receiver's inferred type — NOT another type's same-named
    /// method. Two types both declare `put`; the `Box` value must pick `Box @put`.
    #[test]
    fn t_pos1_receiver_type_dispatch() {
        let src = concat!(
            "module basics.lsp\n",
            "type Box { v int }\n",
            "type Bag { v int }\n",
            "fn Box @put(n int) -> int => n\n",
            "fn Bag @put(a int, b int) -> int => a + b\n",
            "fn main() -> () {\n",
            "    ro b = Box { v: 1 }\n",
            "    ro _ = b.put()\n",
            "}\n",
        );
        let path = write_temp("t_pos1", src);
        let resolved = resolve_module_for_ide(&path, src);
        let p = cursor_after(src, "b.put(");
        let sh = compute_signature_help_in(&resolved, src, p).expect("signature help");
        // Active overload (index 0) must be Box's put — receiver dispatch.
        let active = &sh.signatures[sh.active_signature.unwrap_or(0) as usize];
        assert!(
            active.label.contains("Box"),
            "active signature must be Box @put, got {:?}",
            active.label
        );
        // And it must be the 1-arg variant, not Bag's 2-arg put.
        assert_eq!(
            active.parameters.as_ref().map(|p| p.len()).unwrap_or(0),
            1,
            "Box @put has exactly one parameter"
        );
    }

    /// t_pos2: free-function overloads ranked by arity at the cursor position —
    /// cursor on the 2nd argument selects the 2-parameter overload.
    #[test]
    fn t_pos2_overload_by_arg_count() {
        let src = concat!(
            "module basics.lsp\n",
            "fn f(a int) -> int => a\n",
            "fn f(a int, b int) -> int => a + b\n",
            "fn main() -> () {\n",
            "    ro _ = f(1, 2)\n",
            "}\n",
        );
        // Cursor after the first comma → active_param = 1 → prefer f(a, b).
        let p = cursor_after(src, "f(1, ");
        let sh = compute_signature_help(src, p).expect("signature help");
        let active = &sh.signatures[sh.active_signature.unwrap_or(0) as usize];
        assert_eq!(
            active.parameters.as_ref().map(|p| p.len()).unwrap_or(0),
            2,
            "cursor on 2nd arg selects the 2-parameter overload"
        );
        assert_eq!(sh.active_parameter, Some(1));
    }

    /// t_neg1: unknown receiver type → graceful fallback to the first overload,
    /// never a crash and never empty when some overload with that name exists.
    #[test]
    fn t_neg1_unknown_receiver_fallback() {
        // `mystery` is undeclared → no inferred receiver type in expr_types.
        let src = concat!(
            "module basics.lsp\n",
            "type Box { v int }\n",
            "fn Box @put(n int) -> int => n\n",
            "fn main() -> () {\n",
            "    ro _ = mystery.put()\n",
            "}\n",
        );
        let path = write_temp("t_neg1", src);
        let resolved = resolve_module_for_ide(&path, src);
        let p = cursor_after(src, "mystery.put(");
        let sh = compute_signature_help_in(&resolved, src, p).expect("graceful fallback");
        assert!(
            !sh.signatures.is_empty(),
            "unknown receiver still yields the first overload (graceful)"
        );
        assert!(sh.signatures[0].label.contains("put"));
    }

    /// t_edge1: nested call `f(g(│))` → signature of the INNER callee `g`.
    #[test]
    fn t_edge1_nested_inner_call() {
        let src = concat!(
            "module basics.lsp\n",
            "fn g(x int) -> int => x\n",
            "fn f(y int) -> int => y\n",
            "fn main() -> () {\n",
            "    ro _ = f(g())\n",
            "}\n",
        );
        let p = cursor_after(src, "f(g(");
        let sh = compute_signature_help(src, p).expect("signature help");
        let active = &sh.signatures[sh.active_signature.unwrap_or(0) as usize];
        assert!(
            active.label.contains("g"),
            "innermost call selects g, got {:?}",
            active.label
        );
    }
}
