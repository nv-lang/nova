//! Standard-library search-path index — Plan 104.10 Ф.5 ([M-104.10-hardcode-lists]).
//!
//! compiler-conventions §3: *"what lives in packages is never hardcoded"*. This
//! module replaces the stale hand-maintained tables that used to live in
//! `completion.rs` (`STD_MODULES`) and `code_actions.rs`
//! (`known_stdlib_type_module` / `known_stdlib_protocol_import`) with a real
//! filesystem walk of the resolved stdlib directory
//! ([`nova_codegen::manifest::resolve_std_path`]).
//!
//! It answers three tooling questions, all sourced from disk (never a literal):
//! - **import completion:** what modules exist under a given path prefix
//!   (`import std.│`)?
//! - **add-import quick-fix:** which stdlib module declares type `T` / protocol
//!   `P`?
//!
//! The walk is comparatively expensive (it reads every `.nv` header), so callers
//! build one index per stdlib directory and cache it (see
//! `WorkspaceState::stdlib_index_for`).

use std::collections::{BTreeSet, HashMap};
use std::path::Path;

/// A filesystem-derived view of a package's importable surface.
#[derive(Debug, Default, Clone)]
pub struct StdlibIndex {
    /// Every importable module path, dot-joined (`"std.collections.hashmap"`).
    /// Both directories (namespaces / folder-modules) and standalone `.nv`
    /// file-modules are present.
    modules: BTreeSet<String>,
    /// Exported type name → the module path that declares it.
    type_module: HashMap<String, String>,
    /// Exported protocol name → the module path that declares it.
    protocol_module: HashMap<String, String>,
    /// Package root segment (e.g. `"std"`).
    pkg: String,
}

impl StdlibIndex {
    /// Build the index by walking `stdlib_dir`, attributing every module path to
    /// package root `pkg` (usually `"std"`). Never panics: unreadable dirs/files
    /// are skipped and simply contribute nothing.
    pub fn build(stdlib_dir: &Path, pkg: &str) -> Self {
        let mut idx = StdlibIndex {
            pkg: pkg.to_string(),
            ..Default::default()
        };
        idx.walk(stdlib_dir, &[]);
        idx
    }

    /// Recursively walk `dir`, whose path (relative to the stdlib root) is
    /// `rel_segments`. Bounded depth guards against pathological trees.
    fn walk(&mut self, dir: &Path, rel_segments: &[String]) {
        if rel_segments.len() > 16 {
            return; // defensive depth bound
        }
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            let name = match path.file_name().and_then(|s| s.to_str()) {
                Some(n) => n,
                None => continue,
            };
            // Private / hidden entries are never importable.
            if name.starts_with('_') || name.starts_with('.') {
                continue;
            }
            if path.is_dir() {
                let mut segs = rel_segments.to_vec();
                segs.push(name.to_string());
                self.modules.insert(self.module_path(&segs));
                self.walk(&path, &segs);
            } else if path.extension().and_then(|s| s.to_str()) == Some("nv") {
                let stem = match path.file_stem().and_then(|s| s.to_str()) {
                    Some(s) => s,
                    None => continue,
                };
                // Test peers are not an importable surface.
                if stem.ends_with("_test") {
                    continue;
                }
                // A folder-module peer is imported via its FOLDER, not its file
                // name; attribute its declarations to the folder module path.
                let module_path = if nova_codegen::imports::is_folder_module_peer(&path) {
                    self.module_path(rel_segments)
                } else {
                    let mut segs = rel_segments.to_vec();
                    segs.push(stem.to_string());
                    let mp = self.module_path(&segs);
                    self.modules.insert(mp.clone());
                    mp
                };
                self.scan_decls(&path, &module_path);
            }
        }
    }

    /// Join `pkg` + relative segments into a dotted module path.
    fn module_path(&self, rel_segments: &[String]) -> String {
        if rel_segments.is_empty() {
            self.pkg.clone()
        } else {
            format!("{}.{}", self.pkg, rel_segments.join("."))
        }
    }

    /// Scan a `.nv` file for top-level exported `type` / `protocol`
    /// declarations, recording `name → module_path`. Line-based (no full parse)
    /// for speed and robustness on partially-valid stdlib during editing.
    fn scan_decls(&mut self, path: &Path, module_path: &str) {
        let src = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => return,
        };
        for line in src.lines() {
            // Only column-0 declarations are top-level module items; anything
            // indented is a nested/local declaration and not import-relevant.
            if line.starts_with(char::is_whitespace) {
                continue;
            }
            let mut rest = line.trim_end();
            // Strip visibility prefixes.
            for pfx in ["export ", "pub "] {
                if let Some(r) = rest.strip_prefix(pfx) {
                    rest = r.trim_start();
                }
            }
            // Nova declares protocols as `type Name protocol { … }` (a type decl
            // with a `protocol` kind modifier after the name), not `protocol
            // Name`. Both spellings are handled.
            let decl_rest = rest
                .strip_prefix("type ")
                .or_else(|| rest.strip_prefix("protocol "));
            if let Some(r) = decl_rest {
                if let Some(name) = leading_type_ident(r) {
                    // Everything after the name up to the body `{` — a `protocol`
                    // keyword there marks a protocol declaration.
                    let after_name = &r[r.find(&name).map_or(0, |i| i + name.len())..];
                    let head = after_name.split('{').next().unwrap_or(after_name);
                    let is_protocol = rest.starts_with("protocol ")
                        || head.split_whitespace().any(|t| t == "protocol");
                    if is_protocol {
                        self.protocol_module
                            .entry(name.clone())
                            .or_insert_with(|| module_path.to_string());
                    }
                    // A protocol is also an importable type; register it in both
                    // maps so type-position lookups resolve too.
                    self.type_module
                        .entry(name)
                        .or_insert_with(|| module_path.to_string());
                }
            }
        }
    }

    /// Import completion: given a dotted path prefix (already split into
    /// segments), return the set of next-segment names available under it.
    ///
    /// - `[]` → the package root (`"std"`).
    /// - `["std"]` → top-level modules under the stdlib root.
    /// - `["std","collections"]` → modules under `std.collections.*`.
    /// - a prefix outside this package → empty.
    pub fn child_segments(&self, prefix: &[String]) -> Vec<String> {
        if prefix.is_empty() {
            return vec![self.pkg.clone()];
        }
        let prefix_str = prefix.join(".");
        let mut out = BTreeSet::new();
        for module in &self.modules {
            if let Some(rest) = module.strip_prefix(&prefix_str) {
                // Must be a strict sub-path: the char after the prefix is a dot.
                if let Some(tail) = rest.strip_prefix('.') {
                    if let Some(seg) = tail.split('.').next() {
                        if !seg.is_empty() {
                            out.insert(seg.to_string());
                        }
                    }
                }
            }
        }
        out.into_iter().collect()
    }

    /// The module that declares exported type `name`, if any.
    pub fn type_module(&self, name: &str) -> Option<&str> {
        self.type_module.get(name).map(String::as_str)
    }

    /// The module that declares exported protocol `name`, if any.
    pub fn protocol_module(&self, name: &str) -> Option<&str> {
        self.protocol_module.get(name).map(String::as_str)
    }

    /// Total number of indexed modules (used by tests / diagnostics).
    pub fn module_count(&self) -> usize {
        self.modules.len()
    }
}

/// Extract the leading type/protocol identifier from the text following the
/// `type`/`protocol` keyword. Stops at the first non-identifier char (space,
/// `[`, `{`, `(`, etc.). Returns `None` if the first char is not identifier-start.
fn leading_type_ident(s: &str) -> Option<String> {
    let s = s.trim_start();
    let mut end = 0;
    for (i, c) in s.char_indices() {
        if c.is_ascii_alphanumeric() || c == '_' {
            end = i + c.len_utf8();
        } else {
            break;
        }
    }
    if end == 0 {
        return None;
    }
    let ident = &s[..end];
    if ident.chars().next().map_or(true, |c| c.is_ascii_digit()) {
        return None;
    }
    Some(ident.to_string())
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Locate the repo root (CARGO_MANIFEST_DIR = .../nova-lsp → parent).
    fn stdlib_dir() -> PathBuf {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let repo = manifest.parent().expect("nova-lsp has a parent");
        nova_codegen::manifest::resolve_std_path(repo)
    }

    fn index() -> StdlibIndex {
        StdlibIndex::build(&stdlib_dir(), "std")
    }

    /// POS: real top-level modules are discovered; stale ones are not.
    #[test]
    fn pos_top_level_modules_real_not_stale() {
        let idx = index();
        let top = idx.child_segments(&["std".to_string()]);
        assert!(top.contains(&"collections".to_string()), "collections exists: {top:?}");
        assert!(top.contains(&"encoding".to_string()), "encoding exists");
        assert!(top.contains(&"net".to_string()), "net exists");
        // Written since this test was: the index follows the filesystem, so a
        // module that appears must start being advertised without an edit here.
        assert!(top.contains(&"io".to_string()), "io exists (std/src/io)");
        assert!(top.contains(&"math".to_string()), "math exists (std/src/math)");
        // Stale entries the old hardcoded list advertised must stay ABSENT.
        assert!(!top.contains(&"sync".to_string()), "std.sync does not exist");
        // Private (_experimental) is never surfaced.
        assert!(!top.contains(&"_experimental".to_string()), "_experimental hidden");
    }

    /// POS: nested prefix lists real sub-modules; `map` (never existed) absent.
    #[test]
    fn pos_collections_submodules() {
        let idx = index();
        let subs = idx.child_segments(&["std".to_string(), "collections".to_string()]);
        assert!(subs.contains(&"vec".to_string()), "vec: {subs:?}");
        assert!(subs.contains(&"hash_map".to_string()), "hash_map");
        assert!(subs.contains(&"set".to_string()), "set");
        assert!(!subs.contains(&"map".to_string()), "collections.map never existed");
        // The pre-rename spelling must not come back through a stale list.
        assert!(!subs.contains(&"hashmap".to_string()), "hashmap was renamed to hash_map");
    }

    /// POS: empty prefix yields the package root only.
    #[test]
    fn pos_empty_prefix_is_pkg_root() {
        let idx = index();
        assert_eq!(idx.child_segments(&[]), vec!["std".to_string()]);
    }

    /// POS: a known exported type resolves to a real module path on disk.
    #[test]
    fn pos_type_module_resolves() {
        let idx = index();
        // StringBuilder lives under std.runtime.* on disk.
        let m = idx.type_module("StringBuilder");
        assert!(m.is_some(), "StringBuilder should resolve to a module");
        assert!(m.unwrap().starts_with("std."), "module is under std: {m:?}");
    }

    /// NEG: an unknown prefix yields nothing.
    #[test]
    fn neg_unknown_prefix_empty() {
        let idx = index();
        assert!(idx.child_segments(&["nonexistent_pkg".to_string()]).is_empty());
    }

    /// NEG: an unknown type does not resolve.
    #[test]
    fn neg_unknown_type_unresolved() {
        let idx = index();
        assert!(idx.type_module("NoSuchTypeXYZ").is_none());
    }

    /// EDGE: leading_type_ident stops at brackets and rejects junk.
    #[test]
    fn edge_leading_ident() {
        assert_eq!(leading_type_ident("Vec[T] {").as_deref(), Some("Vec"));
        assert_eq!(leading_type_ident("Foo effect {").as_deref(), Some("Foo"));
        assert_eq!(leading_type_ident("  Spaced").as_deref(), Some("Spaced"));
        assert!(leading_type_ident("[bad]").is_none());
        assert!(leading_type_ident("").is_none());
    }
}
