//! Registry 221.1 #1456: a method that `Option` / `Result` do not have is a checker error.
//!
//! Before: `Some(6).unwrap_or(0)` (retracted, D86 amendment 2026-07-07), `o.no_such_method(1)`
//! with `o Option[int]`, `os[2].unwrap_or(0)` over `Vec[Option[int]]`, `Some(Some(3)).flatten()`
//! -- `nova check` was green and the C compiler refused the program (`member reference type
//! 'NovaOpt_nova_int' is not a pointer`, `no member named ...`), or the call slid into another
//! type's method. The instance-overload check judges only the receivers it can type cheaply and
//! only types whose method set is complete in the extern signatures; a sum declared in Nova
//! source (`Option`, `Result`, the other prelude sums, a program's own) fell through both.
//!
//! The method set of `Option` and `Result` is: the methods declared for them in the prelude, the
//! bare-`T` blankets (`fn[T] T @m`) and the compiler's intrinsics -- nothing else answers a call
//! on them. The receiver is typed by the full inference (the checker channel when it is silent),
//! so every receiver shape is judged: a binding, a constructor call, an index, a field.
//! `unwrap` is retracted too (D85/D86) but is still served by the emitter (the map-spread
//! desugaring synthesises `.get(k).unwrap()`); refusing it is a decision of its own.
//!
//! WHY ONLY `Option`/`Result` (measured on the corpus before enabling): for any other sum the
//! inference is not trusted enough to refuse on it -- `mut acc = Empty` in
//! std/src/collections/linkedlist.nv is inferred as the prelude `ParseBoolError` (whose variant
//! is also `Empty`) instead of `LinkedList[T]`, and the methods a `#impl(Equal + Hash + Clone)`
//! sum gets by synthesis (`m126_sum_rich_autoderive.nv`: `clone`) are not in the method table.
//! `Option` is the only sum with `Some`/`None`, `Result` the only one with `Ok`/`Err`, and
//! neither derives -- refusing there cannot be a false alarm of either kind.

use super::*;

impl<'a> TypeCheckCtx<'a> {
    pub(super) fn check_sum_method_exists(
        &self,
        obj: &Expr,
        method: &str,
        scope: &HashMap<String, TypeRef>,
        span: crate::diag::Span,
        errors: &mut Vec<Diagnostic>,
    ) {
        if method == "unwrap" {
            return;
        }
        // The full inference first; the checker channel when it is silent (an element
        // read out of `Vec[Option[int]]` by index is typed there).
        let channel = || {
            obj.id.is_set().then(|| self.resolved_types_buf.borrow().get(&obj.id).cloned()).flatten()
                .and_then(|rt| Self::resolved_to_typeref(&rt, obj.span))
        };
        let Some(t) = self.infer_expr_type(obj, scope).or_else(channel) else { return };
        let peeled = t.strip_modifiers();
        let TypeRef::Named { path, .. } = peeled else { return };
        let Some(name) = path.last() else { return };
        if !matches!(name.as_str(), "Option" | "Result") {
            return;
        }
        let declared = self.sig.method_table.get(name.as_str()).is_some_and(|m| m.contains_key(method));
        if declared
            || self.prefix_generic_method_exists(peeled, method)
            || crate::codegen::emit_c::CEmitter::primitive_instance_method_known(name, method)
        {
            return;
        }
        errors.push(Diagnostic::new(
            format!(
                "[E7320] no field or method `{method}` on type `{name}` -- `{name}` has the methods declared \
                 for it in the prelude, the bare-`T` blankets and the intrinsics, nothing else (`x ?? v` \
                 replaces the retracted `unwrap_or`, D86)"
            ),
            span,
        ));
    }
}
