//! Registry 221.1 #1640: the result type of a function VALUE passed to a
//! generic method.
//!
//! A generic method whose type parameter appears ONLY in the result of a
//! function-typed parameter -- `fn[T] []T @map[U](f fn(T) -> U) -> []U` --
//! had `U` inferred from the BODY of a closure literal only (`ClosureLight`
//! / `ClosureFull`, emit_c.rs, `closure_return_generics`). A function value
//! of any other form -- a parameter (`keys.map(f)`), a local
//! (`ro g fn(int) -> int = ..; xs.map(g)`), a named function
//! (`xs.map(named_f)`) -- left `U` unbound, and the honest refusal
//! `E7001 closure-arg return type (U)` followed. Measured over
//! map/filter/fold/any/all/position x literal/param/local/named x
//! plain/generic enclosing function: only `map` failed -- the only one whose
//! type parameter lives nowhere but in the callback's result (`fold` binds
//! `Acc` from its initial value, the others return `bool`).
//!
//! The value's own type carries the answer: the checker records it in the
//! `resolved_types` channel as `Func { ret, .. }`, and `ret` is `U`.

use crate::ast::Expr;
use crate::types::ResolvedType;

impl super::CEmitter {
    /// The C type of the result of the function value `e`, from the checker's
    /// channel; `None` when the channel has no function type for it.
    pub(super) fn fn_value_return_c(&self, e: &Expr) -> Option<String> {
        if !e.id.is_set() {
            return None;
        }
        let mut rt = self.resolved_types.get(&e.id)?;
        while let ResolvedType::Readonly(inner) = rt {
            rt = inner;
        }
        let ResolvedType::Func { ret, .. } = rt else { return None };
        let c = self.resolved_type_to_c(ret).ok()?;
        (!c.is_empty() && c != "void*").then_some(c)
    }
}
