//! №666 (carrier #662 layer 2, 2026-08-15): a VARIANT CONSTRUCTOR takes
//! ownership of a `consume` argument -- `Ok(r)`, `Some(r)`, `Err(r)`, any
//! user `Variant(r)` -- exactly like a consume-param call does, so the
//! auto-cleanup flag of the moved binding must be DISARMED at that site.
//! Before this file, `disarm_auto_cleanup_receiver_call` knew two doors only
//! (consuming receiver method; consume-param position of a free fn/method):
//! a variant ctor is neither, so `consume r = open(); Ok(r)` ran `@cleanup`
//! on `r` at the fn's exit AND handed the (dead) value out inside `Ok` -- the
//! receiver owned a corpse. In the http_proxy_chain bridge that corpse was
//! the freshly dialed upstream TcpStream (fd open, TCP CLOSED), the tunnel
//! died right after a successful SOCKS handshake.
//!
//! Split out of `emit_c.rs` per the arch-ratchet rule (`emit_detach.rs` /
//! `variant_ctor_channel.rs` precedent). Called from the ONE trailing/stmt
//! disarm choke point, `disarm_auto_cleanup_receiver_call`, as its third arm.

use super::CEmitter;
use crate::ast::{Expr, ExprKind};

impl CEmitter {
    /// If `e` is a bare-Ident call whose callee names a sum variant (schema
    /// registry -- builtin Option/Result included, user sums included), disarm
    /// the auto-cleanup flag of every `Ident` argument that is a live
    /// consume-binding: ownership moved into the constructor. Conservative:
    /// a name that is NOT a known variant is left alone (a real free fn is
    /// handled by the consume-param-position arm, never here); a payload
    /// argument that is not a bare Ident cannot be a tracked binding.
    pub(super) fn disarm_auto_cleanup_variant_ctor_args(&mut self, e: &Expr) {
        let ExprKind::Call { func, args, .. } = &e.kind else { return };
        let ExprKind::Ident(name) = &func.kind else { return };
        // The callee must be a variant name; a colliding free fn wins the
        // other arm (its positions table) -- if it has one, this is not a ctor.
        if self.free_fn_consume_param_positions.contains_key(name) {
            return;
        }
        if self.sum_schema_registry.variant_sum_candidates(name).is_empty() {
            return;
        }
        for a in args.iter() {
            if let ExprKind::Ident(arg_name) = &a.expr().kind {
                if let Some(var) = self.disarm_var_for(arg_name) {
                    self.line(&format!(
                        "{} = 0;  /* #666: consume moved into variant ctor `{}` -- cleanup disarmed */",
                        var, name));
                }
            }
        }
    }
}
