//! Registry 221.1 #1545/#1527 (and #1533 before them): a bare TYPE name read by
//! the KIND its position admits -- the one door, so no emitter site asks "is this
//! name generic / a type" of a bare-name set on its own.
//!
//! The defect class: the emitter knew a type by its bare name only, and a name is
//! not unique in a CU. `Outcome` is at once a program's value record and the
//! prelude's generic SUM `Outcome[T]` (D455); `Query` is at once polaris' generic
//! record `Query[T]` and a VARIANT `DuckError.Query(str)` of another package.
//! Measured:
//! - #1545 / #1533: a record literal `Outcome { .. }` matched `generic_types` by its
//!   bare name and was built as `Outcome[int]` -- `void* .. /* unknown type
//!   Outcome____nova_int */`, or zeros printed silently for literal elements of a
//!   `[]Outcome`. A record literal builds a RECORD; a sum is never its target.
//! - #1527: the receiver `Query[int]` of `Query[int].from_request(req)` was typed as
//!   the bare EXPRESSION `Query` -- the variant's sum `DuckError` -- so the by-ref
//!   ABI key of the static method was `(DuckError, from_request)` and the large
//!   value argument went by value where the callee takes a pointer (CC-FAIL). The
//!   base of a turbofish in receiver position is a TYPE; a variant is never there.
//!
//! The MODULE half of the pair stays where it lives: a colliding plain type is
//! qualified by `ref_type_base` (D381), which the callers apply after this door.

use super::CEmitter;
use crate::ast::{Expr, ExprKind, TypeDeclKind};

/// The kind a position admits for a bare type name.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum TypeRole {
    /// A record literal `Name { .. }`: only a record is built there.
    RecordLiteral,
    /// The receiver of a static call in type position: `Name[A].m(..)`.
    StaticReceiver,
}

impl CEmitter {
    /// Is `name`, at a position of `role`, the GENERIC template of that kind?
    /// `false` for a name whose generic template is of another kind (the prelude's
    /// sum `Outcome[T]` read at a record literal), so the plain declaration of the
    /// name is used instead.
    pub(super) fn generic_template_for(&self, name: &str, role: TypeRole) -> bool {
        if !self.generic_types.contains(name) {
            return false;
        }
        let Some(t) = self.generic_type_templates.get(name) else { return false };
        match role {
            TypeRole::RecordLiteral => matches!(t.kind, TypeDeclKind::Record(_)),
            TypeRole::StaticReceiver => true,
        }
    }

    /// The type a receiver in TYPE position names -- the base of a turbofish
    /// (`Query[int]` of `Query[int].from_request(..)`) when that name is a type of
    /// the CU (generic or plain). `None` for anything else: the caller types the
    /// receiver as an expression, as before.
    pub(super) fn type_position_name(&self, obj: &Expr) -> Option<String> {
        let ExprKind::TurboFish { base, .. } = &obj.kind else { return None };
        let ExprKind::Ident(name) = &base.kind else { return None };
        if self.generic_template_for(name, TypeRole::StaticReceiver) || self.record_schemas.contains_key(name) {
            Some(name.clone())
        } else {
            None
        }
    }
}
