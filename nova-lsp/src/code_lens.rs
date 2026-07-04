//! `textDocument/codeLens` — Plan 104.10 Ф.20 (run-test / references /
//! implementations) — 🏆 differentiator.
//!
//! V1 advertised `code_lens_provider` but produced *only* the Plan 123.5.1
//! field-cache lens ("N caches inserted"). Ф.20 adds the three lenses users
//! actually expect from a production server, all backed by **real indexes** (no
//! placeholder counts):
//!
//! 1. **run-test** — a `▶ Run test` lens over every `test "…"` block whose
//!    command is the server-side [`CMD_RUN_TEST`] `workspace/executeCommand`
//!    (dispatched in [`crate::server`]) that shells out to `nova test <file>
//!    --filter <name>`. Only `test` items get it (a plain `fn` never does).
//! 2. **references** — `N references` over every `fn` / `type`, the count taken
//!    from the Ф.12 incremental [`ReferencesIndex`] (identical source of truth to
//!    `textDocument/references`, so the numbers always agree). The command is the
//!    de-facto-standard client navigation `editor.action.showReferences` carrying
//!    the pre-resolved locations.
//! 3. **implementations** — `N implementations` over every `protocol`, the count
//!    from the Ф.19 AST implementer scan
//!    ([`crate::type_definition::protocol_implementations_by_name`]) — explicit
//!    `#impl(P)` opt-ins **and** structural conformers, cross-file through the
//!    provenance `file_map`.
//!
//! EDGE: a symbol with no usages still renders `0 references`, and a protocol
//! with no implementers still renders `0 implementations` — the count is never
//! hidden.

use std::path::Path;

use nova_codegen::ast::{Item, TypeDeclKind};
use nova_codegen::diag::{Span, MAIN_FILE_ID};
use ropey::Rope;
use serde_json::json;
use tower_lsp::lsp_types::{CodeLens, Command, Location, Range, Url};

use crate::diagnostic_mapping::byte_offset_to_position;
use crate::provenance::ResolvedModule;
use crate::symbols::ReferencesIndex;
use crate::type_definition;

/// Server-side command id for the run-test lens. Advertised in
/// `execute_command_provider` and handled in `Backend::execute_command`.
pub const CMD_RUN_TEST: &str = "nova.runTest";

/// Client-side navigation command carrying pre-resolved locations. Standard
/// across VS Code language servers (rust-analyzer, gopls) — the editor peeks the
/// locations without a further server round-trip.
const CMD_SHOW_REFERENCES: &str = "editor.action.showReferences";

/// Build the Ф.20 navigation lenses (run-test / references / implementations) for
/// the entry file of `resolved`.
///
/// Only the entry file's own items get lenses (imported items live in other
/// buffers): they are the `resolved.module.items[items_start..]` slice, and each
/// is additionally guarded to carry [`MAIN_FILE_ID`] so its span maps onto `src`.
///
/// `file_path` is the on-disk path of the open buffer; when `None` (an unsaved /
/// non-`file:` URI) the run-test lens is omitted (nothing to hand `nova test`),
/// but references/implementations lenses still render.
pub fn compute_navigation_lenses(
    src: &str,
    uri: &Url,
    file_path: Option<&Path>,
    resolved: &ResolvedModule,
    refs_index: &ReferencesIndex,
) -> Vec<CodeLens> {
    let rope = Rope::from_str(src);
    let module = &resolved.module;
    let mut out: Vec<CodeLens> = Vec::new();

    let start = resolved.items_start.min(module.items.len());
    for item in &module.items[start..] {
        match item {
            Item::Test(td) => {
                if td.span.file_id != MAIN_FILE_ID {
                    continue;
                }
                if let Some(path) = file_path {
                    out.push(run_test_lens(&rope, td.span, &td.name, path));
                }
            }
            Item::Fn(fd) => {
                if fd.span.file_id != MAIN_FILE_ID {
                    continue;
                }
                out.push(references_lens(&rope, src, uri, fd.span, &fd.name, refs_index));
            }
            Item::Type(td) => {
                if td.span.file_id != MAIN_FILE_ID {
                    continue;
                }
                out.push(references_lens(&rope, src, uri, td.span, &td.name, refs_index));
                if matches!(td.kind, TypeDeclKind::Protocol { .. }) {
                    if let Some(lens) =
                        implementations_lens(&rope, src, uri, td.span, &td.name, resolved)
                    {
                        out.push(lens);
                    }
                }
            }
            _ => {}
        }
    }

    out
}

/// `▶ Run test` lens anchored at the declaration line, dispatching the server
/// command with `[file_path, test_name]`.
fn run_test_lens(rope: &Rope, span: Span, test_name: &str, path: &Path) -> CodeLens {
    let range = decl_anchor_range(rope, span);
    CodeLens {
        range,
        command: Some(Command {
            title: "▶ Run test".to_string(),
            command: CMD_RUN_TEST.to_string(),
            arguments: Some(vec![
                json!(path.to_string_lossy().to_string()),
                json!(test_name),
            ]),
        }),
        data: None,
    }
}

/// `N references` lens. The count is the Ф.12 index answer **excluding the
/// declaration itself** — matching `textDocument/references` with
/// `includeDeclaration = false`, so lens and find-references always agree and a
/// never-used symbol legitimately shows `0 references`.
fn references_lens(
    rope: &Rope,
    src: &str,
    uri: &Url,
    span: Span,
    name: &str,
    refs_index: &ReferencesIndex,
) -> CodeLens {
    let name_range = name_range(rope, src, span, name);
    let decl_loc = Location { uri: uri.clone(), range: name_range };
    let locs = refs_index.find(name, Some(&decl_loc), /* include_declaration = */ false);
    let count = locs.len();
    let title = format!("{count} reference{}", if count == 1 { "" } else { "s" });
    CodeLens {
        range: name_range,
        command: Some(Command {
            title,
            command: CMD_SHOW_REFERENCES.to_string(),
            arguments: Some(vec![
                json!(uri.as_str()),
                json!(name_range.start),
                json!(locs),
            ]),
        }),
        data: None,
    }
}

/// `N implementations` lens over a protocol. Count from the Ф.19 AST scan
/// (explicit `#impl` + structural conformers, cross-file). Returns `None` only if
/// the name unexpectedly fails to resolve as a protocol (it always should here).
fn implementations_lens(
    rope: &Rope,
    src: &str,
    uri: &Url,
    span: Span,
    name: &str,
    resolved: &ResolvedModule,
) -> Option<CodeLens> {
    let locs = type_definition::protocol_implementations_by_name(resolved, src, uri, name)?;
    let count = locs.len();
    let name_range = name_range(rope, src, span, name);
    let title = format!("{count} implementation{}", if count == 1 { "" } else { "s" });
    Some(CodeLens {
        range: name_range,
        command: Some(Command {
            title,
            command: CMD_SHOW_REFERENCES.to_string(),
            arguments: Some(vec![
                json!(uri.as_str()),
                json!(name_range.start),
                json!(locs),
            ]),
        }),
        data: None,
    })
}

/// A zero-width range at the start of the declaration span's line — the anchor a
/// lens attaches to (the run-test lens has no meaningful "name" token to sit on,
/// so it hangs above the whole declaration).
fn decl_anchor_range(rope: &Rope, span: Span) -> Range {
    let start = byte_offset_to_position(rope, span.start);
    Range { start, end: start }
}

/// LSP [`Range`] of the identifier `name` inside `span` in `src`.
///
/// Word-boundary search within the declaration span (mirrors
/// `symbols::name_range_in_span`) so `type User` anchors on `User`, not the `type`
/// keyword. Falls back to the span start if not found (e.g. a `test "…"` whose
/// display name is a quoted string that never appears verbatim as an identifier).
fn name_range(rope: &Rope, src: &str, span: Span, name: &str) -> Range {
    let bytes = src.as_bytes();
    let start = (span.start as usize).min(bytes.len());
    let end = (span.end as usize).min(bytes.len());
    let is_ident = |b: u8| b.is_ascii_alphanumeric() || b == b'_';

    if !name.is_empty() && start < end {
        let name_bytes = name.as_bytes();
        let nlen = name_bytes.len();
        let region = &bytes[start..end];
        let mut pos = 0;
        while pos + nlen <= region.len() {
            if &region[pos..pos + nlen] == name_bytes {
                let before_ok = pos == 0 || !is_ident(region[pos - 1]);
                let after_ok =
                    pos + nlen >= region.len() || !is_ident(region[pos + nlen]);
                if before_ok && after_ok {
                    let abs_start = start + pos;
                    let abs_end = abs_start + nlen;
                    return Range {
                        start: byte_offset_to_position(rope, abs_start),
                        end: byte_offset_to_position(rope, abs_end),
                    };
                }
            }
            pos += 1;
        }
    }

    let anchor = byte_offset_to_position(rope, start);
    Range { start: anchor, end: anchor }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provenance;
    use std::path::PathBuf;
    use tower_lsp::lsp_types::Position;

    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("nova-lsp has a parent")
            .to_path_buf()
    }

    fn write_fixture(stem: &str, name: &str, src: &str) -> (Url, PathBuf) {
        let dir = repo_root().join("target").join("f20_codelens_test").join(stem);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("{name}.nv"));
        std::fs::write(&path, src).unwrap();
        let uri = Url::from_file_path(&path).expect("valid file URI");
        (uri, path)
    }

    fn run_lens_titles(lenses: &[CodeLens]) -> Vec<String> {
        lenses
            .iter()
            .filter_map(|l| l.command.as_ref())
            .filter(|c| c.command == CMD_RUN_TEST)
            .map(|c| c.title.clone())
            .collect()
    }

    fn lens_with_title_containing<'a>(
        lenses: &'a [CodeLens],
        needle: &str,
    ) -> Option<&'a CodeLens> {
        lenses
            .iter()
            .find(|l| l.command.as_ref().is_some_and(|c| c.title.contains(needle)))
    }

    /// POS: a run-test lens sits over a `test "…"` block and its command runs
    /// `nova test` with the file path + test name as arguments.
    #[test]
    fn run_test_lens_over_test_block() {
        let src = "\
module app.mod
fn helper() => ()
test \"adds two numbers\" {
  ro x = 1
}
";
        let (uri, path) = write_fixture("run_test", "app", src);
        let resolved = provenance::resolve_module_for_ide(&path, src);
        let idx = ReferencesIndex::default();
        idx.index_file(uri.clone(), src);

        let lenses =
            compute_navigation_lenses(src, &uri, Some(path.as_path()), &resolved, &idx);

        let run = lens_with_title_containing(&lenses, "Run test")
            .expect("a run-test lens over the `test` block");
        let cmd = run.command.as_ref().unwrap();
        assert_eq!(cmd.command, CMD_RUN_TEST);
        let args = cmd.arguments.as_ref().expect("run-test carries [path, name]");
        assert_eq!(args.len(), 2);
        assert_eq!(args[0].as_str().unwrap(), path.to_string_lossy());
        assert_eq!(args[1].as_str().unwrap(), "adds two numbers");
    }

    /// NEG: a plain (non-test) `fn` gets no run-test lens.
    #[test]
    fn non_test_fn_has_no_run_lens() {
        let src = "\
module app.mod
fn helper() => ()
";
        let (uri, path) = write_fixture("no_run", "app", src);
        let resolved = provenance::resolve_module_for_ide(&path, src);
        let idx = ReferencesIndex::default();
        idx.index_file(uri.clone(), src);

        let lenses =
            compute_navigation_lenses(src, &uri, Some(path.as_path()), &resolved, &idx);
        assert!(
            run_lens_titles(&lenses).is_empty(),
            "no run-test lens for a plain fn, got {:?}",
            run_lens_titles(&lenses)
        );
        // But `helper` still gets a references lens.
        assert!(
            lens_with_title_containing(&lenses, "reference").is_some(),
            "helper fn should carry a references lens"
        );
    }

    /// POS: the `N references` count matches what the Ф.12 index answers for
    /// `textDocument/references` (includeDeclaration = false).
    #[test]
    fn references_count_matches_find() {
        let src = "\
module app.mod
fn used() => ()
fn caller() {
  used()
  used()
}
";
        let (uri, path) = write_fixture("refs_count", "app", src);
        let resolved = provenance::resolve_module_for_ide(&path, src);
        let idx = ReferencesIndex::default();
        idx.index_file(uri.clone(), src);

        let lenses =
            compute_navigation_lenses(src, &uri, Some(path.as_path()), &resolved, &idx);

        // Lens title for `used`: two call-sites, declaration excluded → "2 references".
        let used_lens = lenses
            .iter()
            .find(|l| {
                l.command
                    .as_ref()
                    .is_some_and(|c| c.command == CMD_SHOW_REFERENCES && c.title.contains("reference"))
                    && l.range.start.line == 1
            })
            .expect("references lens over `used`");
        assert_eq!(
            used_lens.command.as_ref().unwrap().title,
            "2 references",
            "usages excluding declaration"
        );

        // Independent cross-check against the index the references handler uses:
        // `used` is declared on line 1 at cols 3..7.
        let decl = Location {
            uri: uri.clone(),
            range: Range {
                start: Position { line: 1, character: 3 },
                end: Position { line: 1, character: 7 },
            },
        };
        let found = idx.find("used", Some(&decl), false);
        assert_eq!(found.len(), 2, "index agrees the count is 2");
    }

    /// EDGE: a symbol with zero usages renders `0 references` (not hidden).
    #[test]
    fn zero_references_still_rendered() {
        let src = "\
module app.mod
fn lonely() => ()
";
        let (uri, path) = write_fixture("zero_refs", "app", src);
        let resolved = provenance::resolve_module_for_ide(&path, src);
        let idx = ReferencesIndex::default();
        idx.index_file(uri.clone(), src);

        let lenses =
            compute_navigation_lenses(src, &uri, Some(path.as_path()), &resolved, &idx);
        let lonely = lenses
            .iter()
            .find(|l| l.range.start.line == 1)
            .expect("references lens over `lonely`");
        assert_eq!(lonely.command.as_ref().unwrap().title, "0 references");
    }

    /// POS: `N implementations` over a protocol, count = implementers from the
    /// Ф.19 scan (explicit `#impl` + structural).
    #[test]
    fn implementations_count_over_protocol() {
        let src = "\
module app.mod
type Greetable protocol {
  @greet() -> str
}
#impl(Greetable)
type Dog {
  name str
}
fn Dog @greet() -> str => \"woof\"
type Cat {
  name str
}
fn Cat @greet() -> str => \"meow\"
fn main() => ()
";
        let (uri, path) = write_fixture("impl_count", "app", src);
        let resolved = provenance::resolve_module_for_ide(&path, src);
        let idx = ReferencesIndex::default();
        idx.index_file(uri.clone(), src);

        let lenses =
            compute_navigation_lenses(src, &uri, Some(path.as_path()), &resolved, &idx);
        let impl_lens = lens_with_title_containing(&lenses, "implementation")
            .expect("an implementations lens over the protocol");
        // Dog (explicit #impl) + Cat (structural) = 2.
        assert_eq!(
            impl_lens.command.as_ref().unwrap().title,
            "2 implementations"
        );
    }

    /// EDGE: a protocol with no implementers still renders `0 implementations`.
    #[test]
    fn zero_implementations_still_rendered() {
        let src = "\
module app.mod
type Lonely protocol {
  @ping() -> str
}
fn main() => ()
";
        let (uri, path) = write_fixture("zero_impl", "app", src);
        let resolved = provenance::resolve_module_for_ide(&path, src);
        let idx = ReferencesIndex::default();
        idx.index_file(uri.clone(), src);

        let lenses =
            compute_navigation_lenses(src, &uri, Some(path.as_path()), &resolved, &idx);
        let impl_lens = lens_with_title_containing(&lenses, "implementation")
            .expect("an implementations lens over the protocol");
        assert_eq!(
            impl_lens.command.as_ref().unwrap().title,
            "0 implementations"
        );
    }
}
