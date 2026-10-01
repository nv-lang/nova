//! Registry 221.1 #1419: `import m.{f as g}` names `m.f` as `g` in the
//! IMPORTING file only.
//!
//! The import resolver used to rename the declaration itself (`f` became `g`
//! in the merged unit), so `m`'s own calls to `f` and every other importer's
//! `f` pointed at a symbol nobody emitted, and a file with its own `f` could
//! not reach `m.f` through the alias at all (#1234: the checker bound
//! `net_serve(...)` to the file's own facade `serve`).
//!
//! Now the declaration keeps its name. `alpha_rename` -- the one pass that
//! already knows which Idents are locals -- rewrites every free Ident `g` of
//! the importing file to `f` and records the reference here, keyed by span
//! (`Module::import_alias_refs`); the checker (`types/fn_visibility.rs`)
//! resolves a recorded reference to the imported module's `f` and writes the
//! callee channel the emitter already follows.
//!
//! Types are the child module's (#1444, [`type_rewrite`]): a type alias keeps
//! the declaration too, and TYPE positions of the importing file are rewritten
//! there. A re-export with an alias (`export import m.{f as g}`) keeps it as
//! well (#1444): `g` is the facade's public name, so every file importing `g`
//! from the facade gets the same rewrite as a file that wrote the alias.

use crate::ast::{Import, ImportAliasRef, ImportAnchor, Item, Module, PeerFile};
use crate::diag::FileId;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

mod type_rewrite;
pub use type_rewrite::rewrite_type_aliases;

/// Alias -> the import it came from, per importing file.
pub type FileAliases = HashMap<FileId, HashMap<String, ImportAliasRef>>;

/// Per importing file: alias -> the import it came from. Only selective
/// imports with an `as`, and only aliases of a fn/const (a type's is #1444's).
pub fn file_aliases(module: &Module) -> FileAliases {
    let types = type_rewrite::type_names(module);
    aliases_where(module, &|name| !types.contains(name))
}

/// Per importing file, every alias of a declared name `keep` accepts: the
/// file's own `{x as y}` (plain or `export import`), and the names it imports
/// from a facade that re-exported something under an alias (#1444). A
/// re-export's reference records the FACADE's import path, which names the
/// declaring module.
pub(crate) fn aliases_where(module: &Module, keep: &dyn Fn(&str) -> bool) -> FileAliases {
    let mut out = FileAliases::new();
    let own = |imports: &[Import], acc: &mut HashMap<String, ImportAliasRef>| {
        for imp in imports {
            for it in imp.items.iter().flatten() {
                let Some(alias) = &it.alias else { continue };
                if alias != &it.name && keep(&it.name) {
                    acc.insert(
                        alias.clone(),
                        ImportAliasRef { alias: alias.clone(), name: it.name.clone(), path: imp.path.clone() },
                    );
                }
            }
        }
    };
    if module.peer_files.is_empty() {
        let mut acc = HashMap::new();
        own(&module.imports, &mut acc);
        if !acc.is_empty() {
            out.insert(crate::diag::MAIN_FILE_ID, acc);
        }
        return out;
    }
    // Per facade file: public name -> (declared name, the facade's import path).
    let mut reexports: Vec<(&PeerFile, HashMap<&str, (&str, &[String])>)> = Vec::new();
    for pf in &module.peer_files {
        let mut m = HashMap::new();
        for imp in pf.imports.iter().filter(|imp| imp.is_export) {
            for it in imp.items.iter().flatten() {
                if let Some(alias) = &it.alias {
                    if alias != &it.name && keep(&it.name) {
                        m.insert(alias.as_str(), (it.name.as_str(), imp.path.as_slice()));
                    }
                }
            }
        }
        if !m.is_empty() {
            reexports.push((pf, m));
        }
    }
    for pf in &module.peer_files {
        let mut acc = HashMap::new();
        own(&pf.imports, &mut acc);
        for imp in &pf.imports {
            for (facade, public) in &reexports {
                if facade.file_id == pf.file_id || !import_names_file(imp, pf, facade) {
                    continue;
                }
                for (pub_name, (decl, path)) in public {
                    let seen_as = match &imp.items {
                        None => Some(pub_name.to_string()),
                        Some(sel) => sel
                            .iter()
                            .find(|it| it.name == *pub_name)
                            .map(|it| it.alias.clone().unwrap_or_else(|| it.name.clone())),
                    };
                    if let Some(seen_as) = seen_as {
                        acc.insert(
                            seen_as.clone(),
                            ImportAliasRef { alias: seen_as, name: decl.to_string(), path: path.to_vec() },
                        );
                    }
                }
            }
        }
        if !acc.is_empty() {
            out.insert(pf.file_id, acc);
        }
    }
    out
}

/// Does `imp`, written in `importer`, name the module of file `target`? By the
/// declared module name, by the file system for a relative import, and by the
/// path suffix for a package import (the checker's W6 rule, №705).
fn import_names_file(imp: &Import, importer: &PeerFile, target: &PeerFile) -> bool {
    if imp.path.is_empty() {
        return false;
    }
    if target.module_name == imp.path {
        return true;
    }
    let target_stem: PathBuf = target.path.with_extension("");
    let target_dir: Option<&Path> = target.path.parent();
    match imp.anchor {
        ImportAnchor::Relative { up } => {
            let Some(mut base) = importer.path.parent().map(Path::to_path_buf) else { return false };
            for _ in 0..up {
                if !base.pop() {
                    return false;
                }
            }
            let joined: PathBuf = imp.path.iter().fold(base, |p, s| p.join(s));
            joined == target_stem || target_dir == Some(joined.as_path())
        }
        ImportAnchor::Package => {
            let ends = |p: &Path| {
                let comps: Vec<String> =
                    p.components().map(|c| c.as_os_str().to_string_lossy().to_string()).collect();
                comps.len() >= imp.path.len() && comps[comps.len() - imp.path.len()..] == imp.path[..]
            };
            ends(&target_stem) || target_dir.map_or(false, ends)
        }
    }
}

/// The file a top-level item was declared in.
pub fn item_file(item: &Item) -> FileId {
    match item {
        Item::Fn(f) => f.span.file_id,
        Item::Test(t) => t.span.file_id,
        Item::Bench(b) => b.span.file_id,
        Item::Const(c) => c.span.file_id,
        Item::Type(t) => t.span.file_id,
        Item::Let(l) => l.span.file_id,
        Item::Lemma(l) => l.span.file_id,
    }
}
