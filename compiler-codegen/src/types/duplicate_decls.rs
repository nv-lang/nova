//! Registry 221.1 #1179 / #1183 / #1186: ONE check for a top-level name
//! declared twice inside one module (D281 Rule C: every peer file of a folder
//! is one module).
//!
//! Before this check the refusal lived only in the registration loop of
//! `check_module_impl`, which walks `module.items` -- the entry module's own
//! declarations merged with everything the imports pulled in. That loop asks
//! "was this name seen before?" and, when it had been, excused the collision
//! whenever the name ALSO existed in an imported module (a prelude shadow or a
//! codegen-only merge). So two peer files declaring `Node` were refused, and
//! the same two files were accepted the moment an import pulled in any module
//! that declared its own `Node` -- `std.encoding.serde` did, through a test
//! peer (#1183). A module-level `ro` binding was never judged at all.
//!
//! Here the question is asked where it belongs: only among the declarations
//! the module being compiled wrote itself (`PeerFile::is_entry_module`,
//! `items_here`), grouped by physical module (same `module` name AND same
//! folder, D78: same-named modules in different folders are different
//! modules). Nothing the imports bring in takes part, so the verdict cannot
//! depend on the import set, and `check`/`build`/`test` all reach it through
//! `check_module_impl`.
//!
//! What counts as a duplicate:
//! * type (record, enum, effect, protocol, newtype), `const` and module-level
//!   `ro` share one namespace: any two of them with one name collide;
//! * two functions under one key (free fn by name, method or assoc fn by
//!   `Type.name`) collide only with the SAME signature (D84 overloads by
//!   arity, argument types, modes and result type are legal);
//! * not a duplicate: two `priv(file)` declarations in different files
//!   (D307), two identical `extern` declarations (repeated forward
//!   declarations of one external symbol).
//! A function against a same-named type/const is not judged here: which of
//! those D84/D29 allow is not settled by this check.

use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Type,
    Const,
    Ro,
    Fn,
}

impl Kind {
    fn word(self) -> &'static str {
        match self {
            Kind::Type => "type",
            Kind::Const => "constant",
            Kind::Ro => "module value",
            Kind::Fn => "function",
        }
    }
}

struct Decl<'m> {
    kind: Kind,
    span: Span,
    file_private: bool,
    func: Option<&'m FnDecl>,
}

/// D84: two declarations under one key are the same overload iff receiver
/// mode, linearity marks, arity, argument types and modes, and result type all
/// coincide. Shared with the registration loop in `check_module_impl`.
pub(crate) fn same_overload_sig(a: &FnDecl, b: &FnDecl) -> bool {
    let mode = |f: &FnDecl| f.receiver.as_ref().map(|r| (r.mutable, r.consume)).unwrap_or((false, false));
    if mode(a) != mode(b) || linearity_pair_differs(a, b) {
        return false;
    }
    let args_equal = a.params.len() == b.params.len()
        && a.params.iter().zip(b.params.iter()).all(|(p, q)| {
            typeref_equal(&p.ty, &q.ty) && p.is_mut == q.is_mut && p.consume == q.consume
        });
    args_equal
        && match (&a.return_type, &b.return_type) {
            (None, None) => true,
            (Some(x), Some(y)) => typeref_equal(x, y),
            _ => false,
        }
}

fn decl_of(item: &Item) -> Option<(String, Decl<'_>)> {
    match item {
        Item::Type(t) => Some((t.name.clone(), Decl { kind: Kind::Type, span: t.span, file_private: t.file_private, func: None })),
        Item::Const(c) => Some((c.name.clone(), Decl { kind: Kind::Const, span: c.span, file_private: c.file_private, func: None })),
        Item::Let(l) if !l.is_ghost => {
            let name = match &l.pattern {
                crate::ast::Pattern::Ident { name, .. } => name.clone(),
                crate::ast::Pattern::Variant { path, kind: crate::ast::VariantPatternKind::Unit, .. }
                    if path.len() == 1 => path[0].clone(),
                _ => return None,
            };
            Some((name, Decl { kind: Kind::Ro, span: l.span, file_private: false, func: None }))
        }
        Item::Fn(f) => {
            let key = match &f.receiver {
                Some(r) => format!("{}.{}", r.type_name, f.name),
                None => f.name.clone(),
            };
            Some((key, Decl { kind: Kind::Fn, span: f.span, file_private: f.file_private, func: Some(f) }))
        }
        _ => None,
    }
}

fn collides(a: &Decl, b: &Decl) -> bool {
    if a.file_private && b.file_private && a.span.file_id != b.span.file_id {
        return false;
    }
    match (a.func, b.func) {
        (Some(fa), Some(fb)) => !(fa.is_external && fb.is_external) && same_overload_sig(fa, fb),
        (None, None) => true,
        _ => false,
    }
}

/// Refuse every top-level name the compiled module declares twice. Returns the
/// keys refused, so the registration loop does not refuse them a second time.
///
/// The error sits on the declaration that comes FIRST in load order (the entry
/// file leads it) and the note on the repeat; the message names both files.
/// Anchoring on the first keeps the refusal in the file the user compiled, which
/// is also the only file a fixture's line-pinned `nova:expect` is read from.
pub(super) fn check_duplicate_decls(module: &Module, errors: &mut Vec<Diagnostic>) -> HashSet<String> {
    let file_name = |id: crate::diag::FileId| {
        module
            .peer_files
            .iter()
            .find(|pf| pf.file_id == id)
            .and_then(|pf| pf.path.file_name().map(|n| n.to_string_lossy().into_owned()))
            .unwrap_or_else(|| "this file".to_string())
    };
    // (module name, folder) -> key -> declarations in source order
    let mut groups: HashMap<(&[String], Option<&std::path::Path>), HashMap<String, Vec<Decl>>> = HashMap::new();
    let mut order: Vec<(&[String], Option<&std::path::Path>)> = Vec::new();
    for pf in module.peer_files.iter().filter(|pf| pf.is_entry_module) {
        let gk = (pf.module_name.as_slice(), pf.path.parent());
        if !groups.contains_key(&gk) {
            order.push(gk);
        }
        let g = groups.entry(gk).or_default();
        for it in &pf.items_here {
            if let Some((key, d)) = decl_of(it) {
                g.entry(key).or_default().push(d);
            }
        }
    }
    let mut refused = HashSet::new();
    for gk in order {
        let mut keys: Vec<(&String, &Vec<Decl>)> = groups[&gk].iter().filter(|(_, v)| v.len() > 1).collect();
        keys.sort_by_key(|(_, v)| (v[1].span.file_id, v[1].span.start));
        for (key, decls) in keys {
            for (i, d) in decls.iter().enumerate().skip(1) {
                let Some(first) = decls[..i].iter().find(|e| collides(e, d)) else { continue };
                let head = if d.kind == Kind::Fn {
                    format!("duplicate definition `{}` with same signature", key)
                } else {
                    format!("duplicate top-level name `{}`", key)
                };
                let module_name = if gk.0.is_empty() { String::new() } else { format!(" of module `{}`", gk.0.join(".")) };
                errors.push(
                    Diagnostic::new(
                        format!(
                            "[E_DUPLICATE_DECL] {}: this {} in `{}` and the {} in `{}` are both top-level \
                             declarations{} (D281: all peer files of a folder are one module) -- rename or \
                             remove one of them",
                            head,
                            first.kind.word(),
                            file_name(first.span.file_id),
                            d.kind.word(),
                            file_name(d.span.file_id),
                            module_name,
                        ),
                        first.span,
                    )
                    .with_note_at(format!("`{}` is declared again here", key), d.span),
                );
                refused.insert(key.clone());
            }
        }
    }
    refused
}
