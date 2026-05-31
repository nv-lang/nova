#!/usr/bin/env python3
"""Plan 114 Ф.2: rewrite embedded Nova-body strings in Rust source files.

Only touches contents of `nova_body: Some("...")` literals — embedded
Nova code that should use new keyword syntax after Plan 114 D184.

Usage:
    python scripts/plan114_rust_nova_body.py FILE [FILE ...]
"""
from __future__ import annotations

import re
import sys
from pathlib import Path


# Match `nova_body: Some(` then balanced string literal.
# String literal in Rust: "..." with \" escape sequences and \n newlines.
# Simple approach: match `"..."` non-greedy with escape support.
NOVA_BODY_RE = re.compile(
    r'(nova_body:\s*Some\(\s*)("(?:\\.|[^"\\])*")',
    re.DOTALL,
)


def rewrite_nova_string(s: str) -> str:
    """Apply R1/R2/R7-R9 inside a Nova-body string literal.

    The string is in Rust-escaped form (\\n means newline character,
    `\\"` means escaped quote). We work на logical Nova text by un-escaping,
    rewriting, then re-escaping.
    """
    # Strip surrounding quotes.
    if not (s.startswith('"') and s.endswith('"')):
        return s
    inner = s[1:-1]

    # Decode Rust escapes (just \\n and \\" matter here).
    # Reverse: \\n → \n, \\t → \t, \\\\ → \\, \\" → "
    decoded = (inner
               .replace('\\n', '\n')
               .replace('\\t', '\t')
               .replace('\\"', '"')
               .replace('\\\\', '\\'))

    # Apply rewrites — order matters: let mut before let.
    # `let mut X` → `mut X`
    decoded = re.sub(
        r'(^|[\s;{(\[])let\s+mut\s+',
        lambda m: f"{m.group(1)}mut ",
        decoded,
    )
    # `let X` → `ro X` (after let mut handled)
    decoded = re.sub(
        r'(^|[\s;{(\[])let\s+',
        lambda m: f"{m.group(1)}ro ",
        decoded,
    )
    # `if let CONSTRUCTOR(...)` → `if CONSTRUCTOR(...)` (drop let)
    decoded = re.sub(
        r'\b(if|while|else if)\s+let\s+([A-Z_][\w]*[\s\(\.\[\{]|\(|\{|\[)',
        lambda m: f"{m.group(1)} {m.group(2)}",
        decoded,
    )
    # `if let IDENT = e` → `if ro IDENT = e`
    decoded = re.sub(
        r'\b(if|while|else if)\s+let\s+([a-z_][\w]*)\s*=',
        lambda m: f"{m.group(1)} ro {m.group(2)} =",
        decoded,
    )
    # `readonly` → `ro` (only if standalone keyword, word-boundary)
    decoded = re.sub(r'\breadonly\b', 'ro', decoded)

    # Re-encode escapes for Rust string literal.
    encoded = (decoded
               .replace('\\', '\\\\')
               .replace('"', '\\"')
               .replace('\n', '\\n')
               .replace('\t', '\\t'))

    return '"' + encoded + '"'


def rewrite_file(path: Path) -> bool:
    src = path.read_text(encoding='utf-8')
    new = NOVA_BODY_RE.sub(
        lambda m: m.group(1) + rewrite_nova_string(m.group(2)),
        src,
    )
    if new != src:
        path.write_text(new, encoding='utf-8')
        return True
    return False


def main() -> int:
    if len(sys.argv) < 2:
        print('usage: plan114_rust_nova_body.py FILE [FILE ...]', file=sys.stderr)
        return 1
    changed = 0
    for arg in sys.argv[1:]:
        p = Path(arg)
        if rewrite_file(p):
            print(f"rewrote: {p}")
            changed += 1
        else:
            print(f"unchanged: {p}")
    print(f"changed files: {changed}")
    return 0


if __name__ == '__main__':
    sys.exit(main())
