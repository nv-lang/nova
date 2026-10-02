//! D488 rule 2 (owner, 2026-10-01; plan 172.15 Ф.1-бис): how a `ro` value is passed is
//! decided by its SIZE -- up to three machine words inclusive by copy, larger by a pointer
//! to the caller's place. The threshold is ONE named constant of the compiler for every
//! place that decides the passing (a `ro` parameter, a `ro @` receiver, a `-> ro T`
//! result); a number in an expression is not a threshold (D488 «Правило»). A build key,
//! if one ever appears, is one for the whole program: the oracle and Carina must pass a
//! value the same way, since Carina calls the functions of the shell the oracle emits.
//!
//! Before: `param_is_auto_byref` compared `s > 16` -- the SysV register boundary, which
//! the owner's decision of 2026-08-08 (plan 172.15 п.4) replaced with «at least 24 bytes,
//! as in Swift».

/// Three machine words on a 64-bit target: a `ro` value of at most this many bytes is
/// passed by copy, a larger one by a hidden read-only pointer (D488 rule 2).
pub(super) const VALUE_BYREF_THRESHOLD_BYTES: i64 = 3 * 8;
