// SPDX-License-Identifier: MIT OR Apache-2.0
//
// Format spec for Nova interp-string syntax `${expr:SPEC}`.
//
// Plan 91.14 (D229) shipped V1: `None` (bare `${expr}` → Display.@display) and
// `Debug` (`${expr:?}` → Debug.@debug).
//
// Plan 152.7-B (D258) extends the grammar to a Rust-style format mini-language:
//
//   ${expr:[[fill]align][sign][#][0][width][.precision][type]]}
//
// captured in the `Spec(FormatSpecParsed)` variant. The `None`/`Debug` variants
// are PRESERVED so all existing `matches!(spec, FormatSpec::None | ::Debug)` call
// sites keep working; a bare-debug `${e:?}` still lowers to `FormatSpec::Debug`,
// and a bare `${e}` still lowers to `FormatSpec::None`. Anything with width /
// precision / align / fill / sign / alt / radix lowers to `Spec(..)`.

/// Alignment within the field width.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Align {
    /// `<` — pad on the right (value left-justified).
    Left,
    /// `>` — pad on the left (value right-justified).
    Right,
    /// `^` — pad on both sides (value centered; extra pad goes on the right).
    Center,
}

/// Sign rendering for numeric values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Sign {
    /// (default, no flag) — only `-` for negatives.
    Minus,
    /// `+` — always print a sign (`+` for non-negative, `-` for negative).
    Plus,
}

/// The presentation `type` character at the END of a Rust-style spec.
///
/// `?` (debug) is handled separately (kept as the `FormatSpec::Debug` variant for
/// bare `${e:?}`, and as `Kind::Debug` here when combined with width/align/etc.).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    /// No type char — Display.@display (default).
    Display,
    /// `?` — Debug.@debug.
    Debug,
    /// `x` — lowercase hexadecimal (integers only).
    LowerHex,
    /// `X` — uppercase hexadecimal (integers only).
    UpperHex,
    /// `b` — binary (integers only).
    Binary,
    /// `o` — octal (integers only).
    Octal,
}

impl Kind {
    /// Radix kinds require an integer value and route through the radix C helper.
    pub fn is_radix(&self) -> bool {
        matches!(self, Kind::LowerHex | Kind::UpperHex | Kind::Binary | Kind::Octal)
    }
}

/// A fully-parsed Rust-style format spec.
///
/// All fields are independent and optional; defaults mirror Rust `format!`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FormatSpecParsed {
    /// Fill character (defaults to space). Stored as a Unicode scalar.
    pub fill: char,
    /// Alignment. `None` => default per-kind: numerics right-align, strings
    /// left-align (matching Rust); the formatter resolves the default at emit
    /// time based on the value type.
    pub align: Option<Align>,
    /// Sign flag (`+`). Default = minus-only.
    pub sign: Sign,
    /// `#` alternate form (emits `0x`/`0o`/`0b` radix prefix for integers).
    pub alternate: bool,
    /// `0` zero-pad flag (sign-aware: zeros go between sign/prefix and digits).
    pub zero_pad: bool,
    /// Minimum field width (in *bytes*/columns; ASCII-oriented like Rust V1).
    pub width: Option<usize>,
    /// Precision (`.N`): float decimal places, or string truncation length.
    pub precision: Option<usize>,
    /// Presentation kind / radix.
    pub kind: Kind,
}

impl Default for FormatSpecParsed {
    fn default() -> Self {
        FormatSpecParsed {
            fill: ' ',
            align: None,
            sign: Sign::Minus,
            alternate: false,
            zero_pad: false,
            width: None,
            precision: None,
            kind: Kind::Display,
        }
    }
}

impl FormatSpecParsed {
    /// True when this spec is *equivalent* to the legacy `None` (bare `${e}`).
    pub fn is_trivial_display(&self) -> bool {
        *self == FormatSpecParsed::default()
    }

    /// True when this spec is *equivalent* to the legacy `Debug` (`${e:?}`).
    pub fn is_trivial_debug(&self) -> bool {
        let mut d = FormatSpecParsed::default();
        d.kind = Kind::Debug;
        *self == d
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum FormatSpec {
    /// Bare `${expr}` — calls Display.@display(sb).
    None,
    /// `${expr:?}` — calls Debug.@debug(sb).
    Debug,
    /// `${expr:<rich spec>}` — Plan 152.7-B (D258) Rust-style mini-language.
    Spec(FormatSpecParsed),
}

impl FormatSpec {
    /// Convenience: is this the bare-expr default? (NOTE: `Spec(default)` is
    /// normalized to `None` by the parser, so a `Spec(..)` is never trivial.)
    pub fn is_none(&self) -> bool {
        matches!(self, FormatSpec::None)
    }

    /// Convenience: is this a debug-formatted spec (bare `:?`)?
    pub fn is_debug(&self) -> bool {
        matches!(self, FormatSpec::Debug)
    }
}

impl std::fmt::Display for FormatSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FormatSpec::None => Ok(()),
            FormatSpec::Debug => write!(f, ":?"),
            FormatSpec::Spec(s) => {
                write!(f, ":")?;
                if s.align.is_some() && s.fill != ' ' {
                    write!(f, "{}", s.fill)?;
                }
                match s.align {
                    Some(Align::Left) => write!(f, "<")?,
                    Some(Align::Right) => write!(f, ">")?,
                    Some(Align::Center) => write!(f, "^")?,
                    None => {}
                }
                if matches!(s.sign, Sign::Plus) {
                    write!(f, "+")?;
                }
                if s.alternate {
                    write!(f, "#")?;
                }
                if s.zero_pad {
                    write!(f, "0")?;
                }
                if let Some(w) = s.width {
                    write!(f, "{}", w)?;
                }
                if let Some(p) = s.precision {
                    write!(f, ".{}", p)?;
                }
                match s.kind {
                    Kind::Display => {}
                    Kind::Debug => write!(f, "?")?,
                    Kind::LowerHex => write!(f, "x")?,
                    Kind::UpperHex => write!(f, "X")?,
                    Kind::Binary => write!(f, "b")?,
                    Kind::Octal => write!(f, "o")?,
                }
                Ok(())
            }
        }
    }
}

/// Parse a Rust-style format spec from the RAW text that appears after the `:`
/// inside `${expr:...}`. The caller passes the substring with the leading `:`
/// already stripped. Returns the parsed spec, or an error string carrying a
/// `[E_BAD_FORMAT_SPEC]` (or legacy code) prefix.
///
/// Grammar (Rust-`format!`-derived, locale-independent):
///
/// ```text
/// spec        := (fill? align)? sign? '#'? '0'? width? ('.' precision)? type?
/// fill        := any-char   (only meaningful when followed by an align char)
/// align       := '<' | '>' | '^'
/// sign        := '+'
/// width       := digit+
/// precision   := digit+
/// type        := '?' | 'x' | 'X' | 'b' | 'o'
/// ```
///
/// Notes / deviations from Rust documented in docs/strings.md and D258:
/// - Width by `*` argument and precision by `.*` argument (Rust positional
///   args) are NOT supported — Nova interpolation has no arg-index model.
/// - `e`/`E` (scientific) deferred — `[M-152.7-format-exp]`.
/// - Empty spec (`${e:}`) → `E_FORMAT_SPEC_EMPTY` (preserved from Plan 91.14).
/// - `?` with extra flags is allowed (`${e:>8?}`), unlike bare `${e:?}`.
pub fn parse_rich_format_spec(raw: &str) -> Result<FormatSpec, String> {
    // Empty after `:` → preserve the Plan 91.14 diagnostic.
    if raw.is_empty() {
        return Err(
            "[E_FORMAT_SPEC_EMPTY] format spec after `:` is empty in `${...}`. \
             Expected a format spec, e.g. `?` (debug), `>8` (width+align), \
             `.2` (precision), `x` (hex). Plan 152.7-B (D258)."
                .to_string(),
        );
    }

    // Fast path: bare `?` stays as the legacy Debug variant so existing codegen
    // for `${e:?}` is byte-for-byte unchanged.
    if raw == "?" {
        return Ok(FormatSpec::Debug);
    }

    let chars: Vec<char> = raw.chars().collect();
    let mut i = 0usize;
    let mut spec = FormatSpecParsed::default();

    // --- [[fill]align] ---
    // An align char is one of `<` `>` `^`. A fill char is any char IMMEDIATELY
    // followed by an align char (Rust rule). Detect by looking at positions
    // 0 and 1.
    let is_align = |c: char| c == '<' || c == '>' || c == '^';
    let set_align = |spec: &mut FormatSpecParsed, c: char| match c {
        '<' => spec.align = Some(Align::Left),
        '>' => spec.align = Some(Align::Right),
        '^' => spec.align = Some(Align::Center),
        _ => unreachable!(),
    };
    if chars.len() >= 2 && is_align(chars[1]) {
        // fill + align
        spec.fill = chars[0];
        set_align(&mut spec, chars[1]);
        i = 2;
    } else if !chars.is_empty() && is_align(chars[0]) {
        // align only (default fill)
        set_align(&mut spec, chars[0]);
        i = 1;
    }

    // --- sign --- (only `+`; `-` is the default and Rust rejects an explicit
    // `-` flag, so we do too.)
    if i < chars.len() && chars[i] == '+' {
        spec.sign = Sign::Plus;
        i += 1;
    } else if i < chars.len() && chars[i] == '-' {
        return Err(format!(
            "[E_BAD_FORMAT_SPEC] explicit `-` sign flag is not supported in \
             format spec `{}` — `-` is the default (negatives already show a \
             minus). Use `+` to force a sign. Plan 152.7-B (D258).",
            raw
        ));
    }

    // --- `#` alternate ---
    if i < chars.len() && chars[i] == '#' {
        spec.alternate = true;
        i += 1;
    }

    // --- `0` zero-pad ---
    // The `0` flag is distinguished from a width that starts with `0`: in Rust
    // `08` means zero-pad to width 8. A leading `0` in the width position is the
    // zero-pad flag, and the following digits are the width.
    if i < chars.len() && chars[i] == '0' {
        spec.zero_pad = true;
        i += 1;
    }

    // --- width (digits) ---
    let width_start = i;
    while i < chars.len() && chars[i].is_ascii_digit() {
        i += 1;
    }
    if i > width_start {
        let w: String = chars[width_start..i].iter().collect();
        spec.width = Some(w.parse::<usize>().map_err(|_| {
            format!(
                "[E_BAD_FORMAT_SPEC] width `{}` too large in format spec `{}`. \
                 Plan 152.7-B (D258).",
                w, raw
            )
        })?);
    }

    // --- precision (`.` digits) ---
    if i < chars.len() && chars[i] == '.' {
        i += 1;
        let prec_start = i;
        while i < chars.len() && chars[i].is_ascii_digit() {
            i += 1;
        }
        if i == prec_start {
            return Err(format!(
                "[E_BAD_FORMAT_SPEC] precision `.` must be followed by digits in \
                 format spec `{}` (e.g. `.2`). Plan 152.7-B (D258).",
                raw
            ));
        }
        let p: String = chars[prec_start..i].iter().collect();
        spec.precision = Some(p.parse::<usize>().map_err(|_| {
            format!(
                "[E_BAD_FORMAT_SPEC] precision `{}` too large in format spec `{}`. \
                 Plan 152.7-B (D258).",
                p, raw
            )
        })?);
    }

    // --- type char ---
    if i < chars.len() {
        let t = chars[i];
        match t {
            '?' => spec.kind = Kind::Debug,
            'x' => spec.kind = Kind::LowerHex,
            'X' => spec.kind = Kind::UpperHex,
            'b' => spec.kind = Kind::Binary,
            'o' => spec.kind = Kind::Octal,
            _ => {
                return Err(format!(
                    "[E_FORMAT_SPEC_UNKNOWN] unknown format type `{}` in spec `{}`. \
                     Supported types: `?` (debug), `x`/`X` (hex), `b` (binary), \
                     `o` (octal). For width/precision/align use \
                     `[[fill]align][+][#][0][width][.precision][type]`, e.g. \
                     `>8`, `.2`, `*^10`, `#x`, `08`. Plan 152.7-B (D258).",
                    t, raw
                ));
            }
        }
        i += 1;
    }

    // --- trailing garbage ---
    if i != chars.len() {
        let rest: String = chars[i..].iter().collect();
        return Err(format!(
            "[E_FORMAT_SPEC_TRAILING] unexpected trailing characters `{}` after \
             format spec in `${{...}}`. Valid grammar: \
             `[[fill]align][+][#][0][width][.precision][type]`. \
             Plan 152.7-B (D258).",
            rest
        ));
    }

    // Normalize trivial specs back to the legacy variants so downstream codegen
    // takes the byte-identical fast paths and never sees a no-op rich spec.
    if spec.is_trivial_display() {
        return Ok(FormatSpec::None);
    }
    if spec.is_trivial_debug() {
        return Ok(FormatSpec::Debug);
    }

    // --- semantic validation that doesn't need type info ---
    // Precision combined with a radix type is meaningless (Rust ignores it for
    // integers; we reject to keep the contract honest — precision only applies
    // to floats and strings).
    if spec.precision.is_some() && spec.kind.is_radix() {
        return Err(format!(
            "[E_BAD_FORMAT_SPEC] precision `.{}` is not valid with an integer \
             radix type (`x`/`X`/`b`/`o`) — precision applies to floats \
             (decimal places) and strings (truncation) only. Plan 152.7-B (D258).",
            spec.precision.unwrap()
        ));
    }

    Ok(FormatSpec::Spec(spec))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(raw: &str) -> FormatSpecParsed {
        match parse_rich_format_spec(raw).unwrap() {
            FormatSpec::Spec(s) => s,
            other => panic!("expected Spec, got {:?}", other),
        }
    }

    #[test]
    fn bare_debug_stays_debug() {
        assert_eq!(parse_rich_format_spec("?").unwrap(), FormatSpec::Debug);
    }

    #[test]
    fn empty_is_error() {
        assert!(parse_rich_format_spec("").unwrap_err().contains("E_FORMAT_SPEC_EMPTY"));
    }

    #[test]
    fn width_only() {
        let s = parsed("5");
        assert_eq!(s.width, Some(5));
        assert_eq!(s.align, None);
    }

    #[test]
    fn align_left_width() {
        let s = parsed("<5");
        assert_eq!(s.align, Some(Align::Left));
        assert_eq!(s.width, Some(5));
        assert_eq!(s.fill, ' ');
    }

    #[test]
    fn fill_center() {
        let s = parsed("*^10");
        assert_eq!(s.fill, '*');
        assert_eq!(s.align, Some(Align::Center));
        assert_eq!(s.width, Some(10));
    }

    #[test]
    fn zero_pad_width() {
        let s = parsed("05");
        assert!(s.zero_pad);
        assert_eq!(s.width, Some(5));
    }

    #[test]
    fn precision_float() {
        let s = parsed(".2");
        assert_eq!(s.precision, Some(2));
    }

    #[test]
    fn hex_alt() {
        let s = parsed("#x");
        assert!(s.alternate);
        assert_eq!(s.kind, Kind::LowerHex);
    }

    #[test]
    fn plus_sign() {
        let s = parsed("+");
        assert_eq!(s.sign, Sign::Plus);
    }

    #[test]
    fn upper_hex_width() {
        let s = parsed("08X");
        assert!(s.zero_pad);
        assert_eq!(s.width, Some(8));
        assert_eq!(s.kind, Kind::UpperHex);
    }

    #[test]
    fn unknown_type_rejected() {
        assert!(parse_rich_format_spec("zz").unwrap_err().contains("E_FORMAT_SPEC_UNKNOWN"));
    }

    #[test]
    fn legacy_hex_word_rejected() {
        // `${x:hex}` from plan91_14 t3 must still be rejected.
        assert!(parse_rich_format_spec("hex").is_err());
    }

    #[test]
    fn precision_with_radix_rejected() {
        assert!(parse_rich_format_spec(".2x").unwrap_err().contains("E_BAD_FORMAT_SPEC"));
    }

    #[test]
    fn dot_without_digits_rejected() {
        assert!(parse_rich_format_spec("5.").unwrap_err().contains("E_BAD_FORMAT_SPEC"));
    }

    #[test]
    fn explicit_minus_rejected() {
        assert!(parse_rich_format_spec("-5").unwrap_err().contains("E_BAD_FORMAT_SPEC"));
    }

    #[test]
    fn debug_with_width() {
        let s = parsed(">8?");
        assert_eq!(s.align, Some(Align::Right));
        assert_eq!(s.width, Some(8));
        assert_eq!(s.kind, Kind::Debug);
    }
}
