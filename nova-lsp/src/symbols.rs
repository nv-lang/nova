//! Document/workspace symbols + find-references — Plan 104.4.
//!
//! Implements three LSP features:
//!
//! 1. `textDocument/documentSymbol` — outline (functions, types, tests, methods
//!    nested under their receiver type).
//! 2. `workspace/symbol` — Ctrl+T project-wide symbol search.  Substring +
//!    case-insensitive matching; top-100 pagination.
//! 3. `textDocument/references` — Shift+F12 find-all-usages, cross-file.
//!
//! # V1 simplifications
//!
//! - `documentSymbol`: AST-based (parse only, no typecheck).  Method nesting is
//!   determined by receiver name matching within the same file; cross-file peer
//!   nesting is V2 ([M-104.4-cross-file-method-nesting]).
//! - `workspaceSymbol`: index rebuilt per-file on didChange; no dep-graph.
//! - `references`: full workspace scan per-request via regex word-boundary
//!   matching.  Incremental index is V2 ([M-104.4-refs-incremental-index]).
//! - Fuzzy matching deferred ([M-104.4-workspace-symbol-fuzzy]).

use std::path::Path;
use std::sync::Arc;

use dashmap::DashMap;
use nova_codegen::ast::{FnDecl, Item, Module, Pattern, TypeDeclKind};
use nova_codegen::diag::Span;
use ropey::Rope;
use tower_lsp::lsp_types::{
    DocumentSymbol, Location, Position, Range, SymbolInformation, SymbolKind, Url,
};

use crate::diagnostic_mapping::span_to_range;

// ─────────────────────────────────────────────────────────────────────────────
// Document symbol cache
// ─────────────────────────────────────────────────────────────────────────────

/// Per-URI cache of document symbols.  Invalidated on `didChange`/`didOpen`.
#[derive(Debug, Default)]
pub struct DocumentSymbolCache {
    cache: DashMap<Url, Arc<Vec<DocumentSymbol>>>,
}

impl DocumentSymbolCache {
    /// Retrieve cached symbols for `uri`, or `None` if not present.
    pub fn get(&self, uri: &Url) -> Option<Arc<Vec<DocumentSymbol>>> {
        self.cache.get(uri).map(|r| Arc::clone(&*r))
    }

    /// Store computed symbols for `uri`.
    pub fn insert(&self, uri: Url, symbols: Vec<DocumentSymbol>) {
        self.cache.insert(uri, Arc::new(symbols));
    }

    /// Invalidate cache for `uri` (e.g., on didChange).
    pub fn invalidate(&self, uri: &Url) {
        self.cache.remove(uri);
    }

    /// Clear all cached entries.
    pub fn clear(&self) {
        self.cache.clear();
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Workspace symbol index
// ─────────────────────────────────────────────────────────────────────────────

/// A single indexed symbol entry.
#[derive(Debug, Clone)]
pub struct WorkspaceSymbolEntry {
    pub name: String,
    pub kind: SymbolKind,
    pub uri: Url,
    pub range: Range,
    pub container_name: Option<String>,
}

/// Project-wide symbol index.  Built incrementally: per-file entries stored in a
/// `DashMap<Url, Vec<WorkspaceSymbolEntry>>`.
#[derive(Debug, Default)]
pub struct WorkspaceIndex {
    entries: DashMap<Url, Vec<WorkspaceSymbolEntry>>,
}

impl WorkspaceIndex {
    /// Re-index symbols for one file (call on didChange/didOpen).
    pub fn index_file(&self, uri: Url, src: &str) {
        let entries = index_file_symbols(&uri, src);
        self.entries.insert(uri, entries);
    }

    /// Remove index entries for a closed file.
    pub fn remove_file(&self, uri: &Url) {
        self.entries.remove(uri);
    }

    /// Search symbols matching `query` (case-insensitive substring).
    /// Returns at most `limit` results.
    pub fn search(&self, query: &str, limit: usize) -> Vec<WorkspaceSymbolEntry> {
        let q = query.to_lowercase();
        let mut results: Vec<WorkspaceSymbolEntry> = Vec::new();

        'outer: for file_entries in self.entries.iter() {
            for entry in file_entries.value() {
                if results.len() >= limit {
                    break 'outer;
                }
                if q.is_empty() || entry.name.to_lowercase().contains(&q) {
                    results.push(entry.clone());
                }
            }
        }
        results
    }

    /// Total number of indexed files.
    pub fn file_count(&self) -> usize {
        self.entries.len()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Public functions — documentSymbol
// ─────────────────────────────────────────────────────────────────────────────

/// Compute `DocumentSymbol` outline for `src`.
///
/// Returns empty Vec on parse failure (graceful — no crash, no error to client).
/// Suitable for running in `run_with_large_stack`.
pub fn compute_document_symbols(src: &str) -> Vec<DocumentSymbol> {
    let module = match nova_codegen::parser::parse(src) {
        Ok(m) => m,
        Err(_) => return Vec::new(),
    };
    let rope = Rope::from_str(src);
    build_document_symbols(&module, &rope)
}

// ─────────────────────────────────────────────────────────────────────────────
// Public functions — workspaceSymbol
// ─────────────────────────────────────────────────────────────────────────────

/// Index all symbols in `src` for workspace-wide search.
pub fn index_file_symbols(uri: &Url, src: &str) -> Vec<WorkspaceSymbolEntry> {
    let module = match nova_codegen::parser::parse(src) {
        Ok(m) => m,
        Err(_) => return Vec::new(),
    };
    let rope = Rope::from_str(src);
    build_workspace_entries(uri, &module, &rope)
}

/// Convert `WorkspaceSymbolEntry` list to LSP `SymbolInformation` list.
/// tower-lsp 0.20 uses `SymbolInformation` for workspace/symbol responses.
pub fn entries_to_workspace_symbols(entries: Vec<WorkspaceSymbolEntry>) -> Vec<SymbolInformation> {
    entries
        .into_iter()
        .map(|e| {
            #[allow(deprecated)]
            SymbolInformation {
                name: e.name,
                kind: e.kind,
                tags: None,
                deprecated: None,
                location: Location {
                    uri: e.uri,
                    range: e.range,
                },
                container_name: e.container_name,
            }
        })
        .collect()
}

// ─────────────────────────────────────────────────────────────────────────────
// Public functions — references
// ─────────────────────────────────────────────────────────────────────────────

/// Find all references to `symbol_name` across `files`.
///
/// `files`: list of `(uri, text)` pairs to scan.
/// `include_declaration`: whether to include spans where `symbol_name` is
/// followed by `(` or `:` (declaration context — heuristic for V1).
/// Actually we include all occurrences; the declaration is identified by span.
///
/// Uses word-boundary regex matching to avoid false-positives.
pub fn find_references(
    symbol_name: &str,
    files: &[(Url, String)],
    declaration_location: Option<&Location>,
    include_declaration: bool,
) -> Vec<Location> {
    if symbol_name.is_empty() {
        return Vec::new();
    }

    let mut results: Vec<Location> = Vec::new();

    for (uri, text) in files {
        let matches = find_word_occurrences(text, symbol_name);
        let rope = Rope::from_str(text);

        for (byte_start, byte_end) in matches {
            let range = span_to_range(&rope, byte_start, byte_end);
            let loc = Location { uri: uri.clone(), range };

            // If not including declaration, skip if it matches declaration_location.
            if !include_declaration {
                if let Some(decl) = declaration_location {
                    if &loc.uri == &decl.uri
                        && ranges_overlap(loc.range, decl.range)
                    {
                        continue;
                    }
                }
            }

            results.push(loc);
        }
    }

    results
}

/// Find the position and name of the symbol at `position` in `src`.
///
/// Returns `(symbol_name, declaration_range)` or `None` if position is not
/// on an identifier.
pub fn symbol_at_position(src: &str, position: Position) -> Option<String> {
    let rope = Rope::from_str(src);
    let byte_off = lsp_position_to_byte_offset(&rope, position)?;

    // Walk to find the identifier boundary.
    let bytes = src.as_bytes();
    if byte_off >= bytes.len() {
        return None;
    }

    // Must be on an identifier character.
    if !is_ident_char(bytes[byte_off]) {
        return None;
    }

    // Find start of identifier (walk back).
    let mut start = byte_off;
    while start > 0 && is_ident_char(bytes[start - 1]) {
        start -= 1;
    }

    // Find end of identifier (walk forward).
    let mut end = byte_off;
    while end < bytes.len() && is_ident_char(bytes[end]) {
        end += 1;
    }

    if start == end {
        return None;
    }

    std::str::from_utf8(&bytes[start..end])
        .ok()
        .map(|s| s.to_string())
}

// ─────────────────────────────────────────────────────────────────────────────
// Internal: AST walking for documentSymbol
// ─────────────────────────────────────────────────────────────────────────────

fn build_document_symbols(module: &Module, rope: &Rope) -> Vec<DocumentSymbol> {
    let mut top_level: Vec<DocumentSymbol> = Vec::new();

    // Collect type declarations first so we can attach methods as children.
    // Map: type_name → index in top_level.
    let mut type_indices: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

    for item in &module.items {
        match item {
            Item::Type(td) => {
                let range = span_to_range(rope, td.span.start, td.span.end);
                let selection_range = name_range_in_span(rope, td.span, &td.name);

                let kind = type_decl_kind(&td.kind);

                // Build children: fields, variants, protocol methods.
                let children: Vec<DocumentSymbol> = build_type_children(&td.kind, rope);

                let sym = DocumentSymbol {
                    name: td.name.clone(),
                    detail: type_detail(&td.kind),
                    kind,
                    tags: None,
                    deprecated: None,
                    range,
                    selection_range,
                    children: if children.is_empty() {
                        None
                    } else {
                        Some(children)
                    },
                };
                let idx = top_level.len();
                type_indices.insert(td.name.clone(), idx);
                top_level.push(sym);
            }
            _ => {}
        }
    }

    // Second pass: functions, tests, consts, lets — methods nested under type.
    for item in &module.items {
        match item {
            Item::Fn(fd) => {
                let sym = fn_to_symbol(fd, rope);
                if let Some(receiver) = &fd.receiver {
                    // Method: nest under type if found.
                    let type_name = receiver.type_name.clone();
                    if let Some(&idx) = type_indices.get(&type_name) {
                        if let Some(children) = top_level[idx].children.as_mut() {
                            children.push(sym);
                        } else {
                            top_level[idx].children = Some(vec![sym]);
                        }
                    } else {
                        // Receiver type not in this file — add at top level.
                        top_level.push(sym);
                    }
                } else {
                    top_level.push(sym);
                }
            }
            Item::Test(td) => {
                let range = span_to_range(rope, td.span.start, td.span.end);
                let sel = name_range_in_span(rope, td.span, &td.name);
                #[allow(deprecated)]
                top_level.push(DocumentSymbol {
                    name: td.name.clone(),
                    detail: Some("test".to_string()),
                    kind: SymbolKind::EVENT,
                    tags: None,
                    deprecated: None,
                    range,
                    selection_range: sel,
                    children: None,
                });
            }
            Item::Const(cd) => {
                let range = span_to_range(rope, cd.span.start, cd.span.end);
                let sel = name_range_in_span(rope, cd.span, &cd.name);
                #[allow(deprecated)]
                top_level.push(DocumentSymbol {
                    name: cd.name.clone(),
                    detail: None,
                    kind: SymbolKind::CONSTANT,
                    tags: None,
                    deprecated: None,
                    range,
                    selection_range: sel,
                    children: None,
                });
            }
            Item::Let(ld) => {
                // Extract binding name from the pattern.
                if let Some(name) = pattern_name(&ld.pattern) {
                    let range = span_to_range(rope, ld.span.start, ld.span.end);
                    let sel = name_range_in_span(rope, ld.span, &name);
                    #[allow(deprecated)]
                    top_level.push(DocumentSymbol {
                        name,
                        detail: None,
                        kind: SymbolKind::VARIABLE,
                        tags: None,
                        deprecated: None,
                        range,
                        selection_range: sel,
                        children: None,
                    });
                }
            }
            Item::Bench(bd) => {
                let range = span_to_range(rope, bd.span.start, bd.span.end);
                let sel = name_range_in_span(rope, bd.span, &bd.name);
                #[allow(deprecated)]
                top_level.push(DocumentSymbol {
                    name: bd.name.clone(),
                    detail: Some("bench".to_string()),
                    kind: SymbolKind::EVENT,
                    tags: None,
                    deprecated: None,
                    range,
                    selection_range: sel,
                    children: None,
                });
            }
            Item::Lemma(ld) => {
                let range = span_to_range(rope, ld.span.start, ld.span.end);
                let sel = name_range_in_span(rope, ld.span, &ld.name);
                #[allow(deprecated)]
                top_level.push(DocumentSymbol {
                    name: ld.name.clone(),
                    detail: Some("lemma".to_string()),
                    kind: SymbolKind::FUNCTION,
                    tags: None,
                    deprecated: None,
                    range,
                    selection_range: sel,
                    children: None,
                });
            }
            Item::Type(_) => {} // already handled in first pass
        }
    }

    top_level
}

fn fn_to_symbol(fd: &FnDecl, rope: &Rope) -> DocumentSymbol {
    let range = span_to_range(rope, fd.span.start, fd.span.end);
    let sel = name_range_in_span(rope, fd.span, &fd.name);
    let detail = if fd.receiver.is_some() {
        Some("method".to_string())
    } else {
        None
    };
    #[allow(deprecated)]
    DocumentSymbol {
        name: fd.name.clone(),
        detail,
        kind: if fd.receiver.is_some() {
            SymbolKind::METHOD
        } else {
            SymbolKind::FUNCTION
        },
        tags: None,
        deprecated: None,
        range,
        selection_range: sel,
        children: None,
    }
}

fn type_decl_kind(kind: &TypeDeclKind) -> SymbolKind {
    match kind {
        TypeDeclKind::Record(_) | TypeDeclKind::NamedTuple(_) => SymbolKind::CLASS,
        TypeDeclKind::Sum(_) => SymbolKind::ENUM,
        TypeDeclKind::Effect(_) => SymbolKind::INTERFACE,
        TypeDeclKind::Protocol { .. } => SymbolKind::INTERFACE,
        TypeDeclKind::Newtype(_) => SymbolKind::CLASS,
        TypeDeclKind::Alias(_) => SymbolKind::CLASS,
        TypeDeclKind::Opaque => SymbolKind::CLASS,
    }
}

fn type_detail(kind: &TypeDeclKind) -> Option<String> {
    match kind {
        TypeDeclKind::Sum(_) => Some("sum".to_string()),
        TypeDeclKind::Effect(_) => Some("effect".to_string()),
        TypeDeclKind::Protocol { .. } => Some("protocol".to_string()),
        TypeDeclKind::Newtype(_) => Some("newtype".to_string()),
        TypeDeclKind::Alias(_) => Some("alias".to_string()),
        _ => None,
    }
}

fn build_type_children(kind: &TypeDeclKind, rope: &Rope) -> Vec<DocumentSymbol> {
    let mut children = Vec::new();
    match kind {
        TypeDeclKind::Record(fields) => {
            for field in fields {
                let range = span_to_range(rope, field.span.start, field.span.end);
                let sel = name_range_in_span(rope, field.span, &field.name);
                #[allow(deprecated)]
                children.push(DocumentSymbol {
                    name: field.name.clone(),
                    detail: None,
                    kind: SymbolKind::FIELD,
                    tags: None,
                    deprecated: None,
                    range,
                    selection_range: sel,
                    children: None,
                });
            }
        }
        TypeDeclKind::Sum(variants) => {
            for v in variants {
                let range = span_to_range(rope, v.span.start, v.span.end);
                let sel = name_range_in_span(rope, v.span, &v.name);
                #[allow(deprecated)]
                children.push(DocumentSymbol {
                    name: v.name.clone(),
                    detail: Some("variant".to_string()),
                    kind: SymbolKind::ENUM_MEMBER,
                    tags: None,
                    deprecated: None,
                    range,
                    selection_range: sel,
                    children: None,
                });
            }
        }
        TypeDeclKind::Protocol { methods, .. } | TypeDeclKind::Effect(methods) => {
            for m in methods {
                let range = span_to_range(rope, m.span.start, m.span.end);
                let sel = name_range_in_span(rope, m.span, &m.name);
                #[allow(deprecated)]
                children.push(DocumentSymbol {
                    name: m.name.clone(),
                    detail: Some("method signature".to_string()),
                    kind: SymbolKind::METHOD,
                    tags: None,
                    deprecated: None,
                    range,
                    selection_range: sel,
                    children: None,
                });
            }
        }
        TypeDeclKind::NamedTuple(fields) => {
            for f in fields {
                let range = span_to_range(rope, f.span.start, f.span.end);
                let sel = name_range_in_span(rope, f.span, &f.name);
                #[allow(deprecated)]
                children.push(DocumentSymbol {
                    name: f.name.clone(),
                    detail: None,
                    kind: SymbolKind::FIELD,
                    tags: None,
                    deprecated: None,
                    range,
                    selection_range: sel,
                    children: None,
                });
            }
        }
        _ => {}
    }
    children
}

// ─────────────────────────────────────────────────────────────────────────────
// Internal: AST walking for workspaceSymbol
// ─────────────────────────────────────────────────────────────────────────────

fn build_workspace_entries(uri: &Url, module: &Module, rope: &Rope) -> Vec<WorkspaceSymbolEntry> {
    let mut entries = Vec::new();

    for item in &module.items {
        match item {
            Item::Fn(fd) => {
                let container = fd.receiver.as_ref().map(|r| r.type_name.clone());
                entries.push(WorkspaceSymbolEntry {
                    name: fd.name.clone(),
                    kind: if fd.receiver.is_some() { SymbolKind::METHOD } else { SymbolKind::FUNCTION },
                    uri: uri.clone(),
                    range: span_to_range(rope, fd.span.start, fd.span.end),
                    container_name: container,
                });
            }
            Item::Type(td) => {
                let kind = type_decl_kind(&td.kind);
                entries.push(WorkspaceSymbolEntry {
                    name: td.name.clone(),
                    kind,
                    uri: uri.clone(),
                    range: span_to_range(rope, td.span.start, td.span.end),
                    container_name: None,
                });
            }
            Item::Test(td) => {
                entries.push(WorkspaceSymbolEntry {
                    name: td.name.clone(),
                    kind: SymbolKind::EVENT,
                    uri: uri.clone(),
                    range: span_to_range(rope, td.span.start, td.span.end),
                    container_name: None,
                });
            }
            Item::Const(cd) => {
                entries.push(WorkspaceSymbolEntry {
                    name: cd.name.clone(),
                    kind: SymbolKind::CONSTANT,
                    uri: uri.clone(),
                    range: span_to_range(rope, cd.span.start, cd.span.end),
                    container_name: None,
                });
            }
            Item::Let(ld) => {
                if let Some(name) = pattern_name(&ld.pattern) {
                    entries.push(WorkspaceSymbolEntry {
                        name,
                        kind: SymbolKind::VARIABLE,
                        uri: uri.clone(),
                        range: span_to_range(rope, ld.span.start, ld.span.end),
                        container_name: None,
                    });
                }
            }
            Item::Bench(bd) => {
                entries.push(WorkspaceSymbolEntry {
                    name: bd.name.clone(),
                    kind: SymbolKind::EVENT,
                    uri: uri.clone(),
                    range: span_to_range(rope, bd.span.start, bd.span.end),
                    container_name: None,
                });
            }
            Item::Lemma(ld) => {
                entries.push(WorkspaceSymbolEntry {
                    name: ld.name.clone(),
                    kind: SymbolKind::FUNCTION,
                    uri: uri.clone(),
                    range: span_to_range(rope, ld.span.start, ld.span.end),
                    container_name: None,
                });
            }
        }
    }

    entries
}

// ─────────────────────────────────────────────────────────────────────────────
// Internal: references — word-boundary scan
// ─────────────────────────────────────────────────────────────────────────────

/// Find all word-boundary occurrences of `word` in `text`.
/// Returns `(byte_start, byte_end)` pairs.
pub fn find_word_occurrences(text: &str, word: &str) -> Vec<(usize, usize)> {
    if word.is_empty() {
        return Vec::new();
    }
    let bytes = text.as_bytes();
    let word_bytes = word.as_bytes();
    let wlen = word_bytes.len();
    let mut results = Vec::new();

    let mut i = 0;
    while i + wlen <= bytes.len() {
        if bytes[i..i + wlen] == *word_bytes {
            // Check word boundary: char before must NOT be ident, char after must NOT be ident.
            let before_ok = i == 0 || !is_ident_char(bytes[i - 1]);
            let after_ok = i + wlen >= bytes.len() || !is_ident_char(bytes[i + wlen]);
            if before_ok && after_ok {
                results.push((i, i + wlen));
            }
        }
        i += 1;
    }
    results
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Compute LSP Range for the name of a symbol within its declaration span.
///
/// Scans the source bytes in `[span.start, span.end)` to find the first
/// occurrence of `name` as a word token.  Falls back to `span_to_range` if
/// not found (should not happen for well-formed AST).
fn name_range_in_span(rope: &Rope, span: Span, name: &str) -> Range {
    let src = rope.to_string();
    let bytes = src.as_bytes();
    let start = span.start.min(bytes.len());
    let end = span.end.min(bytes.len());

    if name.is_empty() || start >= end {
        return span_to_range(rope, span.start, span.end);
    }

    let name_bytes = name.as_bytes();
    let nlen = name_bytes.len();
    let search_region = &bytes[start..end];

    let mut pos = 0;
    while pos + nlen <= search_region.len() {
        if search_region[pos..pos + nlen] == *name_bytes {
            let before_ok = pos == 0 || !is_ident_char(search_region[pos - 1]);
            let after_ok = pos + nlen >= search_region.len()
                || !is_ident_char(search_region[pos + nlen]);
            if before_ok && after_ok {
                let abs_start = start + pos;
                let abs_end = abs_start + nlen;
                return span_to_range(rope, abs_start, abs_end);
            }
        }
        pos += 1;
    }

    // Fallback: use the whole declaration span.
    span_to_range(rope, span.start, span.end)
}

/// Convert an LSP `Position` to a byte offset in the rope.
fn lsp_position_to_byte_offset(rope: &Rope, pos: Position) -> Option<usize> {
    let line = pos.line as usize;
    let character = pos.character as usize;

    if line >= rope.len_lines() {
        return None;
    }

    let line_start_char = rope.line_to_char(line);
    let line_end_char = if line + 1 < rope.len_lines() {
        rope.line_to_char(line + 1)
    } else {
        rope.len_chars()
    };

    // Walk UTF-16 code units.
    let mut remaining_cu = character;
    let mut char_idx = line_start_char;
    for ch in rope.slice(line_start_char..line_end_char).chars() {
        if remaining_cu == 0 {
            break;
        }
        let w = ch.len_utf16();
        if remaining_cu < w {
            break;
        }
        remaining_cu -= w;
        char_idx += 1;
    }

    if char_idx > rope.len_chars() {
        return None;
    }

    Some(rope.char_to_byte(char_idx))
}

/// Extract the binding name from a simple `let` pattern (best-effort).
/// Returns `None` for complex patterns (record, tuple, array, wildcard).
fn pattern_name(pattern: &Pattern) -> Option<String> {
    match pattern {
        Pattern::Ident { name, .. } => Some(name.clone()),
        Pattern::Binding { name, .. } => Some(name.clone()),
        _ => None,
    }
}

fn ranges_overlap(a: Range, b: Range) -> bool {
    // Two ranges overlap if neither is completely before the other.
    !(a.end.line < b.start.line
        || (a.end.line == b.start.line && a.end.character <= b.start.character)
        || b.end.line < a.start.line
        || (b.end.line == a.start.line && b.end.character <= a.start.character))
}

// ─────────────────────────────────────────────────────────────────────────────
// Workspace file discovery (reused from compiler.rs pattern)
// ─────────────────────────────────────────────────────────────────────────────

/// Collect all `.nv` files under `root` recursively.
pub fn collect_nv_files(root: &Path) -> Vec<(Url, String)> {
    let mut out = Vec::new();
    collect_nv_files_rec(root, &mut out);
    out
}

fn collect_nv_files_rec(dir: &Path, out: &mut Vec<(Url, String)>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name == "target" || name.starts_with('.') {
                continue;
            }
            collect_nv_files_rec(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("nv") {
            if let Ok(text) = std::fs::read_to_string(&path) {
                if let Ok(uri) = Url::from_file_path(&path) {
                    out.push((uri, text));
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── documentSymbol tests ─────────────────────────────────────────────────

    // pos1: Functions recognized as Function symbols with correct range.
    #[test]
    fn doc_pos1_fn_symbols() {
        // "mytest" is a valid module name (test is a reserved keyword in Nova).
        let src = "module mytest\nfn hello() => ()\nfn world(x int) => x\n";
        let syms = compute_document_symbols(src);
        let names: Vec<&str> = syms.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"hello"), "missing 'hello': {names:?}");
        assert!(names.contains(&"world"), "missing 'world': {names:?}");
        let hello = syms.iter().find(|s| s.name == "hello").unwrap();
        assert_eq!(hello.kind, SymbolKind::FUNCTION);
    }

    // pos2: Types recognized as Class/Enum/Interface symbols.
    #[test]
    fn doc_pos2_type_symbols() {
        let src = "module mytest\ntype Point { x int, y int }\n";
        let syms = compute_document_symbols(src);
        assert!(!syms.is_empty(), "no symbols in file with type");
        let point = syms.iter().find(|s| s.name == "Point").unwrap();
        assert_eq!(point.kind, SymbolKind::CLASS);
        // Fields as children.
        let children = point.children.as_ref().expect("Point should have children");
        let field_names: Vec<&str> = children.iter().map(|c| c.name.as_str()).collect();
        assert!(field_names.contains(&"x"), "missing field 'x'");
        assert!(field_names.contains(&"y"), "missing field 'y'");
    }

    // pos3: Methods nested under their receiver type.
    #[test]
    fn doc_pos3_methods_nested_under_type() {
        let src = "module mytest\ntype Dog { name str }\nfn Dog @bark() => ()\n";
        let syms = compute_document_symbols(src);
        let dog = syms.iter().find(|s| s.name == "Dog").expect("Dog should be in outline");
        let children = dog.children.as_ref().expect("Dog should have children (method)");
        let child_names: Vec<&str> = children.iter().map(|c| c.name.as_str()).collect();
        assert!(child_names.contains(&"bark"), "method 'bark' not nested under Dog: {child_names:?}");
    }

    // pos4: Tests recognized as Event symbols.
    #[test]
    fn doc_pos4_test_symbols() {
        // test blocks use quoted name; body uses `ro x = 1` etc.
        let src = "module mytest\ntest \"basic\" { ro _x = 1 }\n";
        let syms = compute_document_symbols(src);
        let t = syms.iter().find(|s| s.kind == SymbolKind::EVENT);
        assert!(t.is_some(), "test should appear as EVENT symbol");
    }

    // pos5: Mixed outline (fn + type + const).
    #[test]
    fn doc_pos5_mixed_outline() {
        // Use simple body expressions to avoid record-literal parsing ambiguity.
        let src = "module mytest\nconst MAX int = 100\ntype Item { val int }\nfn make_item() => ()\n";
        let syms = compute_document_symbols(src);
        let has_const = syms.iter().any(|s| s.kind == SymbolKind::CONSTANT);
        let has_class = syms.iter().any(|s| s.kind == SymbolKind::CLASS);
        let has_fn = syms.iter().any(|s| s.kind == SymbolKind::FUNCTION);
        assert!(has_const, "should have CONSTANT");
        assert!(has_class, "should have CLASS");
        assert!(has_fn, "should have FUNCTION");
    }

    // pos6: Generic function name doesn't include `[T]`.
    #[test]
    fn doc_pos6_generic_fn_name_clean() {
        let src = "module mytest\nfn identity[T](x T) => x\n";
        let syms = compute_document_symbols(src);
        let f = syms.iter().find(|s| s.name.starts_with("identity"));
        assert!(f.is_some(), "generic fn not found: {:?}", syms.iter().map(|s| &s.name).collect::<Vec<_>>());
        // Name should NOT contain '['.
        let name = &f.unwrap().name;
        assert!(!name.contains('['), "name should not include generics: {name}");
        assert_eq!(name, "identity");
    }

    // neg1: Empty file returns empty Vec (no crash).
    #[test]
    fn doc_neg1_empty_file() {
        let syms = compute_document_symbols("");
        assert!(syms.is_empty(), "empty file should produce no symbols");
    }

    // neg2: Parse error → empty Vec (graceful).
    #[test]
    fn doc_neg2_parse_error_graceful() {
        let syms = compute_document_symbols("this is not valid nova {{{");
        // Should not panic; returns empty.
        assert!(syms.is_empty(), "parse error should return empty symbols, not crash");
    }

    // edge1: Sum type — variants as ENUM_MEMBER children.
    #[test]
    fn doc_edge1_sum_type_variants() {
        let src = "module mytest\ntype Color | Red | Green | Blue\n";
        let syms = compute_document_symbols(src);
        let color = syms.iter().find(|s| s.name == "Color");
        assert!(color.is_some(), "Color should be in outline");
        assert_eq!(color.unwrap().kind, SymbolKind::ENUM);
        let children = color.unwrap().children.as_ref().expect("Color should have variant children");
        let names: Vec<&str> = children.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"Red"), "Red variant missing");
        assert!(names.contains(&"Green"), "Green variant missing");
        assert!(names.contains(&"Blue"), "Blue variant missing");
    }

    // edge2: Only const — only Constant symbols (top-level let/mut are bound by pattern).
    #[test]
    fn doc_edge2_only_const() {
        let src = "module mytest\nconst X int = 1\nconst Y int = 2\n";
        let syms = compute_document_symbols(src);
        assert!(!syms.is_empty(), "should have symbols");
        assert!(syms.iter().all(|s| s.kind == SymbolKind::CONSTANT),
            "all symbols should be CONSTANT: {:?}", syms.iter().map(|s| &s.kind).collect::<Vec<_>>());
    }

    // ── workspaceSymbol tests ────────────────────────────────────────────────

    fn make_uri(name: &str) -> Url {
        Url::parse(&format!("file:///workspace/{name}")).unwrap()
    }

    // pos1: Exact name search finds symbol.
    #[test]
    fn ws_pos1_exact_name_search() {
        let index = WorkspaceIndex::default();
        let uri = make_uri("main.nv");
        // "main" is not a reserved keyword in module context
        let src = "module mymain\nfn compute() => 42\n";
        index.index_file(uri.clone(), src);
        let results = index.search("compute", 100);
        assert!(!results.is_empty(), "exact search should find 'compute'");
        assert_eq!(results[0].name, "compute");
    }

    // pos2: Substring search matches.
    #[test]
    fn ws_pos2_substring_search() {
        let index = WorkspaceIndex::default();
        let uri = make_uri("utils.nv");
        let src = "module myutils\nfn compute_sum() => 0\nfn compute_avg() => 0\n";
        index.index_file(uri.clone(), src);
        let results = index.search("compute", 100);
        assert!(results.len() >= 2, "substring 'compute' should match both fns: {}", results.len());
    }

    // pos3: Empty query returns all symbols (up to limit).
    #[test]
    fn ws_pos3_empty_query_returns_all() {
        let index = WorkspaceIndex::default();
        let uri = make_uri("lib.nv");
        let src = "module mylib\nfn a() => ()\nfn b() => ()\nfn c() => ()\n";
        index.index_file(uri.clone(), src);
        let results = index.search("", 100);
        assert!(results.len() >= 3, "empty query should return all symbols");
    }

    // pos4: Cross-file results include correct URI.
    #[test]
    fn ws_pos4_cross_file_uri() {
        let index = WorkspaceIndex::default();
        let uri_a = make_uri("a.nv");
        let uri_b = make_uri("b.nv");
        index.index_file(uri_a.clone(), "module moda\nfn func_a() => ()\n");
        index.index_file(uri_b.clone(), "module modb\nfn func_b() => ()\n");
        let results = index.search("func_a", 100);
        assert!(!results.is_empty());
        assert_eq!(results[0].uri, uri_a, "should match uri_a");
    }

    // neg1: Non-existent symbol → empty results.
    #[test]
    fn ws_neg1_no_match() {
        let index = WorkspaceIndex::default();
        let uri = make_uri("empty.nv");
        index.index_file(uri.clone(), "module myempty\nfn foo() => ()\n");
        let results = index.search("zzz_nonexistent_zzz", 100);
        assert!(results.is_empty(), "should be empty for non-existent symbol");
    }

    // neg2: Very long query doesn't crash; returns empty.
    #[test]
    fn ws_neg2_long_query_graceful() {
        let index = WorkspaceIndex::default();
        let query = "x".repeat(2000);
        let results = index.search(&query, 100);
        assert!(results.is_empty(), "very long query should return empty");
    }

    // ── references tests ─────────────────────────────────────────────────────

    // pos1: Find usages of function in same file.
    #[test]
    fn ref_pos1_same_file_usages() {
        // Raw text (not module-checked, just searched for word occurrences).
        let text = "fn greet() => ()\nfn run() => greet()\n";
        let uri = make_uri("greet.nv");
        let files = vec![(uri.clone(), text.to_string())];
        let locs = find_references("greet", &files, None, true);
        // Should find both declaration and call site.
        assert!(locs.len() >= 2, "should find at least 2 occurrences of 'greet'");
    }

    // pos2: includeDeclaration=true includes declaration span.
    #[test]
    fn ref_pos2_include_declaration() {
        let text = "fn foo() => foo()";
        let uri = make_uri("a.nv");
        let files = vec![(uri.clone(), text.to_string())];
        let locs = find_references("foo", &files, None, true);
        assert!(locs.len() >= 2, "should include declaration");
    }

    // pos3: includeDeclaration=false excludes declaration span.
    #[test]
    fn ref_pos3_exclude_declaration() {
        // "fn foo() => foo()" — 'foo' at col 3 (decl) and col 13 (use).
        let text = "fn foo() => foo()";
        let uri = make_uri("b.nv");
        let decl_range = Range {
            start: Position { line: 0, character: 3 },
            end: Position { line: 0, character: 6 },
        };
        let decl_loc = Location { uri: uri.clone(), range: decl_range };
        let files = vec![(uri.clone(), text.to_string())];
        let locs = find_references("foo", &files, Some(&decl_loc), false);
        // Should not include the decl location.
        for loc in &locs {
            assert!(
                !ranges_overlap(loc.range, decl_range),
                "excluded declaration should not appear in locs: {locs:?}"
            );
        }
    }

    // pos4: Type name usages.
    #[test]
    fn ref_pos4_type_usages() {
        let text = "type Cat { age int }\nfn make() => Cat { age: 1 }\n";
        let uri = make_uri("c.nv");
        let files = vec![(uri.clone(), text.to_string())];
        let locs = find_references("Cat", &files, None, true);
        assert!(locs.len() >= 2, "Cat should appear at type decl and usage: {locs:?}");
    }

    // pos5: Cross-file references.
    #[test]
    fn ref_pos5_cross_file() {
        let uri_a = make_uri("lib.nv");
        let uri_b = make_uri("run.nv");
        let files = vec![
            (uri_a.clone(), "fn helper() => ()\n".to_string()),
            (uri_b.clone(), "fn run() => helper()\n".to_string()),
        ];
        let locs = find_references("helper", &files, None, true);
        assert!(locs.len() >= 2, "should find helper in both files");
        let uris: Vec<&Url> = locs.iter().map(|l| &l.uri).collect();
        assert!(uris.contains(&&uri_a), "uri_a should appear");
        assert!(uris.contains(&&uri_b), "uri_b should appear");
    }

    // pos6: References in import statement text.
    #[test]
    fn ref_pos6_import_references() {
        let text = "import std.helpers.{Helper}\nfn run(h Helper) => ()\n";
        let uri = make_uri("d.nv");
        let files = vec![(uri.clone(), text.to_string())];
        let locs = find_references("Helper", &files, None, true);
        // Should appear in import line and in fn signature.
        assert!(locs.len() >= 2, "Helper should appear in import + signature");
    }

    // neg1: Non-existent symbol → empty.
    #[test]
    fn ref_neg1_no_usages() {
        let text = "fn foo() => ()\n";
        let uri = make_uri("e.nv");
        let files = vec![(uri.clone(), text.to_string())];
        let locs = find_references("nonexistent_xyz", &files, None, true);
        assert!(locs.is_empty(), "should be empty for non-existent symbol");
    }

    // neg2: Position in whitespace → None.
    #[test]
    fn ref_neg2_whitespace_position() {
        // character 2 is ' ' (space between 'fn' and 'foo') — not an ident.
        let result = symbol_at_position("fn foo() => ()", Position { line: 0, character: 2 });
        assert!(result.is_none(), "space position should return None");
    }

    // edge1: Word boundary — 'foo' in 'foobar' is NOT matched.
    #[test]
    fn ref_edge1_word_boundary() {
        let text = "fn foobar() => ()\nfn foo() => foobar()";
        let uri = make_uri("f.nv");
        let files = vec![(uri.clone(), text.to_string())];
        let locs = find_references("foo", &files, None, true);
        // 'foo' alone appears exactly once (line 1: "fn foo()").
        // 'foobar' should NOT be matched by word-boundary search for 'foo'.
        for loc in &locs {
            assert_ne!(loc.range.start.line, 0,
                "'foo' matched inside 'foobar' on line 0 — word boundary broken");
        }
        assert!(!locs.is_empty(), "'foo' should appear at least once");
    }

    // ── find_word_occurrences unit tests ─────────────────────────────────────

    #[test]
    fn word_occ_simple() {
        let text = "foo bar foo baz";
        let occ = find_word_occurrences(text, "foo");
        assert_eq!(occ.len(), 2, "should find 2 occurrences");
        assert_eq!(occ[0], (0, 3));
        assert_eq!(occ[1], (8, 11));
    }

    #[test]
    fn word_occ_no_partial() {
        let occ = find_word_occurrences("foobar foo_baz", "foo");
        // 'foobar' - 'foo' at start but followed by 'b' (ident) → not matched.
        // 'foo_baz' - 'foo' followed by '_' (ident) → not matched.
        assert!(occ.is_empty(), "partial matches should not be returned: {occ:?}");
    }

    #[test]
    fn word_occ_empty_word() {
        let occ = find_word_occurrences("hello", "");
        assert!(occ.is_empty());
    }

    // ── symbol_at_position tests ─────────────────────────────────────────────

    #[test]
    fn sym_at_pos_on_ident() {
        let src = "fn hello_world() => ()";
        let pos = Position { line: 0, character: 5 }; // 'l' in 'hello_world'
        let result = symbol_at_position(src, pos);
        assert_eq!(result.as_deref(), Some("hello_world"));
    }

    #[test]
    fn sym_at_pos_on_space() {
        let src = "fn hello() => ()";
        let pos = Position { line: 0, character: 2 }; // space
        let result = symbol_at_position(src, pos);
        assert!(result.is_none());
    }
}
