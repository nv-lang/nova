//! Registry 221.1 #1234 (D29, `spec/decisions/07-modules.md`, "Конфликты
//! имён"): "Если импортированное имя совпадает с локальным или другим
//! импортом — ошибка компиляции. Решается алиасом через `as`".
//!
//! Before this check the rule had no enforcement: a file importing
//! `polaris.net.{serve}` while its own module declared a facade `serve` bound
//! every call to the facade (№534 "own module shadows"), and with equal
//! signatures nothing at all was said -- `serve(1)` ran the wrong function.
//!
//! A conflict is judged per importing file, on the name the import puts in
//! scope (the alias when there is one), and within one kind only: a free fn
//! against a free fn, a type against a type, a const against a const. Whether
//! a fn and a same-named type/const collide is not something D29 says; that
//! question is left open rather than decided here.

use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
enum DeclKind {
    Fn,
    Type,
    Const,
}

impl DeclKind {
    fn word(self) -> &'static str {
        match self {
            DeclKind::Fn => "function",
            DeclKind::Type => "type",
            DeclKind::Const => "constant",
        }
    }
}

struct Decl {
    kind: DeclKind,
    span: Span,
    file_private: bool,
}

impl<'a> TypeCheckCtx<'a> {
    /// Whether the declaration at `file` is what import path `path` names.
    fn decl_matches_import(&self, file: crate::diag::FileId, path: &[String]) -> bool {
        self.file_modules.borrow().get(&file).map_or(false, |m| m.as_slice() == path)
            || self.file_paths.get(&file).map_or(false, |p| Self::path_matches_import(p, path))
    }

    pub(super) fn check_import_name_conflicts(&self, module: &Module, errors: &mut Vec<Diagnostic>) {
        if crate::import_alias::conflict_check_disabled() {
            return;
        }
        let mut decls: HashMap<&str, Vec<Decl>> = HashMap::new();
        for it in &module.items {
            let (name, d) = match it {
                Item::Fn(f) if f.receiver.is_none() => {
                    (f.name.as_str(), Decl { kind: DeclKind::Fn, span: f.span, file_private: f.file_private })
                }
                Item::Type(t) => (t.name.as_str(), Decl { kind: DeclKind::Type, span: t.span, file_private: false }),
                Item::Const(c) => (c.name.as_str(), Decl { kind: DeclKind::Const, span: c.span, file_private: false }),
                _ => continue,
            };
            decls.entry(name).or_default().push(d);
        }
        for pf in &module.peer_files {
            let file = pf.file_id;
            // visible name -> (import path, kind, imported declaration span)
            let mut seen: HashMap<String, (Vec<String>, String, DeclKind, Span)> = HashMap::new();
            for imp in &pf.imports {
                for it in imp.items.iter().flatten() {
                    let Some(cands) = decls.get(it.name.as_str()) else { continue };
                    let own_mod = |d: &Decl| {
                        self.same_physical_module(file, d.span.file_id) && (!d.file_private || d.span.file_id == file)
                    };
                    let Some(imported) = cands
                        .iter()
                        .find(|d| !own_mod(d) && !d.file_private && self.decl_matches_import(d.span.file_id, &imp.path))
                    else {
                        continue;
                    };
                    let visible = it.alias.clone().unwrap_or_else(|| it.name.clone());
                    let from = match imp.anchor {
                        crate::ast::ImportAnchor::Package => imp.path.join("."),
                        crate::ast::ImportAnchor::Relative { up: 0 } => format!("./{}", imp.path.join(".")),
                        crate::ast::ImportAnchor::Relative { up } => {
                            format!("{}{}", "../".repeat(up as usize), imp.path.join("."))
                        }
                    };
                    let own = decls
                        .get(visible.as_str())
                        .and_then(|v| v.iter().find(|d| d.kind == imported.kind && own_mod(d)));
                    if let Some(own) = own {
                        errors.push(
                            Diagnostic::new(
                                format!(
                                    "[E_IMPORT_NAME_CONFLICT] imported {} `{}` (from `{}`) has the same name as a {} \
                                     declared in this module -- rename the import: `import {}.{{{} as <other_name>}}` \
                                     (D29, name conflicts)",
                                    imported.kind.word(), visible, from, own.kind.word(), from, it.name,
                                ),
                                it.span,
                            )
                            .with_note_at(format!("`{}` is declared here", visible), own.span),
                        );
                        continue;
                    }
                    match seen.get(&visible) {
                        Some((other_path, other_from, kind, other_span)) if other_path != &imp.path && *kind == imported.kind => {
                            errors.push(
                                Diagnostic::new(
                                    format!(
                                        "[E_IMPORT_NAME_CONFLICT] imported {} `{}` (from `{}`) has the same name as \
                                         the one imported from `{}` -- rename one import with `as` (D29, name \
                                         conflicts)",
                                        imported.kind.word(), visible, from, other_from,
                                    ),
                                    it.span,
                                )
                                .with_note_at(format!("the other `{}` is declared here", visible), *other_span),
                            );
                        }
                        Some(_) => {}
                        None => {
                            seen.insert(visible, (imp.path.clone(), from, imported.kind, imported.span));
                        }
                    }
                }
            }
        }
    }
}
