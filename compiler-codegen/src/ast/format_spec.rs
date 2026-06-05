// SPDX-License-Identifier: MIT OR Apache-2.0
//
// Plan 91.14 (D229) — format spec для Nova interp-string syntax.
//
// V1 ships только `Debug` variant ($expr:?). Future extensions
// (Hex, Pad(usize), Precision(u8), Bin, Oct, Exp) — followup
// [M-91.14-format-dsl-extensions].
//
// Variant `None` = bare ${expr} → routes к Printable.@fmt (D183);
// `Debug` = ${expr:?} → routes к DebugPrintable.@debug_fmt (D229).

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum FormatSpec {
    /// Bare `${expr}` — calls Printable.@fmt(sb).
    /// Default — preserves V1 semantics for code без format-spec annotation.
    None,
    /// `${expr:?}` — calls DebugPrintable.@debug_fmt(sb).
    /// Debug-specific representation: memberwise for structs, quoted+escaped
    /// for strings, hex-address for pointers (unsafe context only).
    Debug,
}

impl FormatSpec {
    /// Convenience: is this the bare-expr default?
    pub fn is_none(&self) -> bool {
        matches!(self, FormatSpec::None)
    }

    /// Convenience: is this a debug-formatted spec?
    pub fn is_debug(&self) -> bool {
        matches!(self, FormatSpec::Debug)
    }
}

impl std::fmt::Display for FormatSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FormatSpec::None => Ok(()),
            FormatSpec::Debug => write!(f, ":?"),
        }
    }
}
