//! Registry 221.1 #971: a type name in an ANNOTATION must be visible from here.
//!
//! [INV-PROPERTY: neg/p971_undeclared_type_return_neg, neg/p971_undeclared_type_param_neg]
//! Before this check `walk_typeref` ended its `TypeRef::Named` arm with
//! "unknown name -- not our business (name-resolution)" and returned: any name
//! that was not in the arity table became an opaque nominal type, and every
//! check in that position went quiet -- `fn takes(v Vecc[int]) -> int => 1`
//! accepted `takes(42)` and printed `1`. Plan 173 F.5 had already closed the
//! same excuse for the record literal (`[E_UNKNOWN_TYPE]`); this is the same
//! door in the annotation positions (parameter, return, field, local, generic
//! argument, `extern` signature, alias, bound), reached through every form of
//! `TypeRef` because `walk_typeref` recurses through all twelve.
//!
//! The visibility test is the record literal's, plus what an annotation can
//! name and a literal cannot: a primitive, a well-known protocol alias in a
//! bound (`[T Ord]`), and a type variable that a BOUND introduces -- D355 §1,
//! `fn[I Next[T]] I @fold`: "`T₁,…,Tₙ` are inferred from the bound, no need to
//! declare them", and `02-types.md` forbids declaring them explicitly
//! (`E_UNUSED_PREFIX_TYPEVAR`). Without the last one the draft of 2026-09-06
//! measured 6512 false refusals out of 6539 on `std/src`.

use super::*;

/// The ONE list of primitive type names (#971 step zero). Before it there were
/// four copies that disagreed: `is_primitive_type_name` (15), the arity table
/// seed in `TypeCheckCtx::build` (15), `is_primitive_scalar_type_name` (14, no
/// `uint`) and the bound check's local list (17, with `any`/`never`).
///
/// `any` and `never` are deliberately NOT here: they are the referential top
/// and bottom types (`arity_exempt`), not primitives, and every site that
/// accepts them names them itself -- the bound check in
/// `check_generic_bound_declarations` keeps them explicitly.
pub(crate) const PRIMITIVE_TYPE_NAMES: &[&str] = &[
    "int", "i8", "i16", "i32", "i64",
    "u8", "u16", "u32", "u64", "uint",
    "f32", "f64", "bool", "char", "str",
];

/// Well-known std protocol aliases (D237 renames and their partners). They are
/// accepted as bound names without a declaration in the CU; before #971 this
/// list lived as a local of `check_generic_bound_declarations` only.
pub(crate) const STDLIB_PROTOCOL_ALIASES: &[&str] = &[
    "Ord", "Eq", "ToStr", "TryFrom", "TryInto",
    "Hash", "Display", "Equal", "Compare", "Clone", "Debug",
    "Iterable", "From", "Into",
];

/// Language types with no `.nv` declaration anywhere: the channel capability
/// halves of D91 (`06-concurrency.md`, "capability-split на `ChanWriter[T]` /
/// `ChanReader[T]`"), which `Channel[T].new(cap)` returns. `Channel` itself is
/// an `external type` in std and resolves like any declared type.
const LANGUAGE_INTRINSIC_TYPES: &[&str] = &["ChanReader", "ChanWriter"];

impl<'a> TypeCheckCtx<'a> {
    /// #971: `name` (the last path segment of a `TypeRef::Named`) is a type a
    /// reader can see from the current file. Called where the arity table has
    /// no entry -- the place that used to return in silence.
    pub(super) fn annotation_type_name_visible(&self, name: &str, gs: &GenericScope) -> bool {
        name == "Self"
            || gs.contains_key(name)
            || arity_exempt(name)
            || PRIMITIVE_TYPE_NAMES.contains(&name)
            || STDLIB_PROTOCOL_ALIASES.contains(&name)
            || LANGUAGE_INTRINSIC_TYPES.contains(&name)
            || self.types_get_here_contains(name)
            || self.sum_variant_names.contains(name)
    }

    /// D355 §1: a name used as an ARGUMENT of a bound (`T` in `fn[I Next[T]]`,
    /// `E` in `Vec[T consume Cleanup[E]]`) and not otherwise visible is a type
    /// variable the bound introduces. Put it into `gs` so the annotation check
    /// sees it as what it is. A name that IS visible (`int`, a declared type)
    /// stays a type -- this only adds what would otherwise be unknown.
    pub(super) fn add_bound_introduced_vars(
        &self,
        generics: &[GenericParam],
        gs: &mut GenericScope,
    ) {
        for g in generics {
            for b in &g.bounds {
                let TypeRef::Named { generics: args, span, .. } = b else { continue };
                let mut names: HashSet<String> = HashSet::new();
                for a in args {
                    Self::collect_named_idents(a, &mut names);
                }
                let mut names: Vec<String> = names.into_iter().collect();
                names.sort();
                for n in names {
                    if self.arity.contains_key(&n) || self.annotation_type_name_visible(&n, gs) {
                        continue;
                    }
                    gs.insert(n.clone(), GenericParam::unbounded(n, *span));
                }
            }
        }
    }

    /// An entry of an EFFECT ROW (`Io`, `Net`, `Fail[E]`). Its head is an effect
    /// name, and the built-in effects (`Io`, `Net`, `Fs`, `Db`, `Time`, `Log`,
    /// ...) resolve without a type declaration in the CU, so the head is not
    /// judged by this check -- an unknown effect head is a separate defect
    /// (registry row filed with #971). Its type ARGUMENTS (`E` in `Fail[E]`) are
    /// ordinary annotation positions and are judged.
    pub(super) fn walk_effect_ref(
        &self,
        e: &TypeRef,
        gs: &GenericScope,
        errors: &mut Vec<Diagnostic>,
    ) {
        if let TypeRef::Named { path, generics, .. } = e {
            if let Some(head) = path.last() {
                if !self.arity.contains_key(head) && !self.annotation_type_name_visible(head, gs) {
                    for g in generics {
                        self.walk_typeref(g, gs, errors);
                    }
                    return;
                }
            }
        }
        self.walk_typeref(e, gs, errors);
    }

    /// The diagnostic. The sentence is true for both a misspelling and a real
    /// type that is declared elsewhere but not imported here (registry row:
    /// "undeclared" would be a lie for the second one), and it says both.
    pub(super) fn unknown_annotation_type_diag(name: &str, span: Span) -> Diagnostic {
        // Plan 133: a REMOVED primitive is unknown too, but its reason is known
        // and was the emitter's sentence before #971 (`resolved_type_to_c`);
        // the checker now says it first, in the same words.
        if name == "usize" || name == "isize" {
            return Diagnostic::new(
                format!("[E_UNKNOWN_TYPE] type `{name}` is removed — use `int` (Plan 133)"),
                span,
            );
        }
        Diagnostic::new(
            format!(
                "[E_UNKNOWN_TYPE] unknown type `{name}`: no type of this name is in scope \
                 here -- it is neither declared in this module nor imported (a misspelling, \
                 or a type from a module this file does not import). An unknown name used to \
                 become an opaque type and switched off every check in its position (#971).",
            ),
            span,
        )
    }
}
