//! [221.1 №250/№251] Resolving the concrete `NovaOpt_<T>` C-type a bare
//! `None` sub-expression must construct, for the one case where the
//! pre-existing "channel-first" lookup (`resolved_types[expr.id]`,
//! `emit_c.rs`'s `ExprKind::Ident` arm, 172.1.2) and its `current_fn_return_ty`
//! fallback BOTH silently miss: `None` NESTED inside `Ok(None)`/`Err(None)`
//! inside a BUILTIN Option/Result Nova-body method drained per-(T,E) from
//! `mono_worklist` (prelude `Option[Result[T, E]] @transpose`'s own
//! `None => Ok(None)` arm is the driving case, §250/§251 fixtures).
//!
//! Why the two pre-existing sources both fail here:
//!   - `resolved_types[expr.id]` is the checker's ONE-TIME result for the
//!     GENERIC TEMPLATE's AST node (T unresolved, `Option[T]`) — the SAME
//!     node id is reused verbatim across every per-(T,E) `mono_worklist`
//!     drain, so it can never reflect the concrete instantiation currently
//!     being emitted.
//!   - `current_fn_return_ty` names the ENCLOSING function's return type —
//!     for `None` nested inside `Ok(..)`, that's the OUTER `Result[..]`, the
//!     wrong type family entirely, not the `Option[..]` `None` itself needs.
//!
//! Both silently fall through to a hardcoded `NovaOpt_nova_int` default that
//! was only ever coincidentally correct when T happens to be `int` — masked
//! for every other (T, E) pair (the exact repro shape of both markers).
//!
//! Fix: `expected_option_elem_hint` (`emit_c.rs` `CEmitter` field) — a
//! context hint propagated the same way `expected_record_type`/
//! `expected_sum_hint` already are, set ONLY around a DIRECT `Ok(None)`/
//! `Err(None)` payload (never a recursively-nested one — same "narrower
//! scoping" discipline the `expected_sum_hint` revert-note calls for, so it
//! cannot leak into an unrelated nested bare-variant construction) from that
//! payload's OWN already-resolved concrete C type (itself derived from THIS
//! mono instantiation's `current_fn_return_ty`, one level up at the
//! `Ok`/`Err` call site) — the only one of the three sources that is
//! correct for every (T, E) pair, not just the historically-hardcoded
//! `(nova_int, nova_str)`.

/// Priority-resolve the `NovaOpt_<T>` C-type for a bare `None`: the
/// context `hint` (see module doc) wins, then the checker `channel` read
/// (already filtered to `NovaOpt_`-shaped by the caller is NOT assumed —
/// filtered here), then a direct enclosing `-> Option[X]` `fn_return_ty`,
/// else the legacy `NovaOpt_nova_int` default.
pub fn resolve_bare_none_novaopt_ty(
    hint: Option<String>,
    channel: Option<String>,
    fn_return_ty: Option<&String>,
) -> String {
    hint
        .or_else(|| channel.filter(|t| t.starts_with("NovaOpt_")))
        .or_else(|| fn_return_ty.filter(|t| t.starts_with("NovaOpt_")).cloned())
        .unwrap_or_else(|| "NovaOpt_nova_int".into())
}

/// The `expected_option_elem_hint` value to set (or clear, restoring the
/// pre-existing behavior) while emitting a `Ok`/`Err` call's single
/// argument — `Some` only when that argument is DIRECTLY a bare `None`
/// (not nested deeper) and the payload's own C-type is `NovaOpt_`-shaped.
pub fn payload_hint_for(payload_c: &str, arg0_is_bare_none: bool) -> Option<String> {
    if arg0_is_bare_none && payload_c.starts_with("NovaOpt_") {
        Some(payload_c.to_string())
    } else {
        None
    }
}
