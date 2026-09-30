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
//! Types are NOT covered: a type alias still renames the declaration in the
//! resolver (`imports.rs`), because type positions are not rewritten. Such an
//! alias is left out of the map below. Neither is a re-export with an alias
//! (`export import m.{f as g}`): `g` is then the facade's public name, seen by
//! files that never wrote the alias, so the resolver keeps renaming for it.

use crate::ast::{ImportAliasRef, Item, Module};
use crate::diag::FileId;
use std::collections::{HashMap, HashSet};

/// Kill switch for the both-ways probe: `NOVA_KILL_1419=1` restores the old
/// declaration rename and the emitter's caller-file-first symbol lookup.
pub fn fix_disabled() -> bool {
    use std::sync::OnceLock;
    static OFF: OnceLock<bool> = OnceLock::new();
    *OFF.get_or_init(|| std::env::var("NOVA_KILL_1419").map(|v| !v.is_empty() && v != "0").unwrap_or(false))
}

/// Kill switch for the D29 import-name conflict (`types/import_conflict.rs`).
pub fn conflict_check_disabled() -> bool {
    use std::sync::OnceLock;
    static OFF: OnceLock<bool> = OnceLock::new();
    *OFF.get_or_init(|| std::env::var("NOVA_KILL_D29_IMPORT").map(|v| !v.is_empty() && v != "0").unwrap_or(false))
}

/// Per importing file: alias -> the import it came from. Only selective
/// imports with an `as`, and only aliases that do not name a (renamed) type.
pub fn file_aliases(module: &Module) -> HashMap<FileId, HashMap<String, ImportAliasRef>> {
    if fix_disabled() {
        return HashMap::new();
    }
    let type_names: HashSet<&str> = module
        .items
        .iter()
        .filter_map(|it| match it {
            Item::Type(t) => Some(t.name.as_str()),
            _ => None,
        })
        .collect();
    let mut out: HashMap<FileId, HashMap<String, ImportAliasRef>> = HashMap::new();
    let mut add = |fid: FileId, imports: &[crate::ast::Import]| {
        for imp in imports.iter().filter(|imp| !imp.is_export) {
            for it in imp.items.iter().flatten() {
                let Some(alias) = &it.alias else { continue };
                if type_names.contains(alias.as_str()) || alias == &it.name {
                    continue;
                }
                out.entry(fid).or_default().insert(
                    alias.clone(),
                    ImportAliasRef { alias: alias.clone(), name: it.name.clone(), path: imp.path.clone() },
                );
            }
        }
    };
    if module.peer_files.is_empty() {
        add(crate::diag::MAIN_FILE_ID, &module.imports);
    } else {
        for pf in &module.peer_files {
            add(pf.file_id, &pf.imports);
        }
    }
    out
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
