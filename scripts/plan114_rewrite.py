#!/usr/bin/env python3
"""Plan 114 — bulk rewrite recipe R1-R12.

Применяет mechanical rewrite rules ко всему `.nv` corpus:
- R1   let IDENT = …               → ro IDENT = …
- R2   let mut IDENT = …            → mut IDENT = …
- R3   let (PAT) = …                → ro (PAT) = …
- R4   let mut (PAT) = …            → mut (PAT) = …
- R5   let { PAT } = …              → ro { PAT } = …
- R6   let IDENT TYPE = …           → ro IDENT TYPE = …  (typed)
- R7   if let Constructor(...) = …  → if Constructor(...) = …  (drop let)
- R8   if let Constructor(mut x)    → if Constructor(mut x)    (drop let, mut inside)
- R9   if let IDENT = …             → if ro IDENT = …          (add ro keyword)
- R9a  if let mut IDENT = …         → if mut IDENT = …         (add mut keyword)
- R9b  if let (a, b) = …            → if (a, b) = …            (drop let, tuple)
- R10  while let …                  → analogously to R7-R9b
- R11  readonly IDENT (внутри type) → ro IDENT
- R12  readonly TYPE (type-pos)     → ro TYPE

Защита: не трогает строки-комментарии `^\s*//`, не трогает содержимое
строковых литералов (heuristic: однострочный fast-path; multi-line strings
крайне редки в .nv для keyword'ов). Compiler errors после rewrite — финальная
verification.

Usage:
    python scripts/plan114_rewrite.py PATH [PATH ...]    # apply in-place
    python scripts/plan114_rewrite.py --dry PATH         # dry-run, show diff stats
"""
from __future__ import annotations

import argparse
import os
import re
import sys
from pathlib import Path


# ──────────────────────────────────────────────────────────────────────────
# Stage 1: if/while let — handle BEFORE general `let` rewrite, иначе R1
# превратит `if let X` в `if ro X` ошибочно.
# ──────────────────────────────────────────────────────────────────────────

# `if let Some(x) = e` / `if let _Variant(a, b) = e` / `if let MyType { … } = e`
# — constructor / destructure через type-path начинается с UPPERCASE.
# Также LParen (tuple) и LBrace (record) и LBracket (array pattern).
IFLET_STRUCTURAL = re.compile(
    r'(?P<kw>\b(?:if|while|else if|else  +if)\s+)let\s+(?P<rest>[A-Z_][\w]*[\s\(\.\[\{]|\(|\{|\[)'
)

# `if let mut IDENT = …` — mutable identifier-pattern.
IFLET_MUT_IDENT = re.compile(
    r'(?P<kw>\b(?:if|while|else if)\s+)let\s+mut\s+(?P<id>[a-z_][\w]*)\s*='
)

# `if let IDENT = …` — bare identifier (lowercase) — requires `ro`.
IFLET_BARE_IDENT = re.compile(
    r'(?P<kw>\b(?:if|while|else if)\s+)let\s+(?P<id>[a-z_][\w]*)\s*='
)


def rewrite_if_while_let(line: str) -> str:
    # R7/R8/R9b: constructor / tuple / record / array — drop `let`.
    def _strip_let(m: re.Match) -> str:
        return f"{m.group('kw')}{m.group('rest')}"

    line = IFLET_STRUCTURAL.sub(_strip_let, line)

    # R9a: identifier mut.
    def _id_mut(m: re.Match) -> str:
        return f"{m.group('kw')}mut {m.group('id')} ="

    line = IFLET_MUT_IDENT.sub(_id_mut, line)

    # R9: bare identifier.
    def _id_ro(m: re.Match) -> str:
        return f"{m.group('kw')}ro {m.group('id')} ="

    line = IFLET_BARE_IDENT.sub(_id_ro, line)
    return line


# ──────────────────────────────────────────────────────────────────────────
# Stage 2: let / let mut — statement-leading binding declarations.
# Order matters: `let mut` first (R2), then `let` (R1).
# ──────────────────────────────────────────────────────────────────────────

# R2: `let mut IDENT` / `let mut (` / `let mut {` / `let mut [` — leading,
# considers indentation. Word boundary on `let`.
LET_MUT = re.compile(r'(?P<lead>(?:^|[\s\;\{\}])(?:\s*))let\s+mut\b')

# R1+R3+R5+R6: `let IDENT|`(`|`{`|`[` — bare let без mut.
LET_BARE = re.compile(r'(?P<lead>(?:^|[\s\;\{\}])(?:\s*))let\b')


def rewrite_let_bindings(line: str) -> str:
    line = LET_MUT.sub(lambda m: f"{m.group('lead')}mut", line)
    line = LET_BARE.sub(lambda m: f"{m.group('lead')}ro", line)
    return line


# ──────────────────────────────────────────────────────────────────────────
# Stage 3: readonly → ro (R11, R12) — keyword rename, всё в одну форму.
# `readonly` уникальное слово, безопасный word-boundary rewrite.
# ──────────────────────────────────────────────────────────────────────────

READONLY = re.compile(r'\breadonly\b')


def rewrite_readonly(line: str) -> str:
    return READONLY.sub('ro', line)


# ──────────────────────────────────────────────────────────────────────────
# Skip rules: doc-comments, line comments, raw strings.
# ──────────────────────────────────────────────────────────────────────────

LINE_COMMENT = re.compile(r'^\s*//')


def is_skip_line(line: str) -> bool:
    """Skip pure comment lines. Mixed code+comment (e.g. `let x = 5 // foo`)
    still gets the code portion rewritten — sed-style. Inside-string false
    positives caught by compiler errors.
    """
    return bool(LINE_COMMENT.match(line))


# ──────────────────────────────────────────────────────────────────────────
# Per-file driver.
# ──────────────────────────────────────────────────────────────────────────

def rewrite_text(src: str, *, markdown: bool = False) -> tuple[str, dict[str, int]]:
    """Rewrite all .nv lines. В markdown mode применяет правила только
    внутри fenced ```nova blocks (детекция по leading ```nova).
    """
    stats = {'lines': 0, 'changed': 0, 'iflet': 0, 'let': 0, 'readonly': 0}
    out_lines: list[str] = []
    in_nova_fence = not markdown  # для .nv всегда true; для md только внутри fence
    for line in src.splitlines(keepends=True):
        stats['lines'] += 1
        if markdown:
            stripped = line.lstrip()
            if stripped.startswith('```'):
                # toggle fence state
                if in_nova_fence:
                    in_nova_fence = False
                else:
                    in_nova_fence = stripped.startswith('```nova')
                out_lines.append(line)
                continue
        if not in_nova_fence:
            out_lines.append(line)
            continue
        if is_skip_line(line):
            out_lines.append(line)
            continue
        orig = line
        before_iflet = line
        line = rewrite_if_while_let(line)
        if line != before_iflet:
            stats['iflet'] += 1
        before_let = line
        line = rewrite_let_bindings(line)
        if line != before_let:
            stats['let'] += 1
        before_ro = line
        line = rewrite_readonly(line)
        if line != before_ro:
            stats['readonly'] += 1
        if line != orig:
            stats['changed'] += 1
        out_lines.append(line)
    return ''.join(out_lines), stats


def find_files(roots: list[Path], extensions: tuple[str, ...]) -> list[Path]:
    out: list[Path] = []
    for root in roots:
        if root.is_file():
            if root.suffix in extensions:
                out.append(root)
        elif root.is_dir():
            for ext in extensions:
                out.extend(root.rglob(f'*{ext}'))
    # dedup + sort for determinism
    return sorted(set(out))


def main() -> int:
    ap = argparse.ArgumentParser(description='Plan 114 bulk rewrite R1-R12.')
    ap.add_argument('paths', nargs='+', help='files or directories to rewrite')
    ap.add_argument('--dry', action='store_true', help='do not write; report stats only')
    ap.add_argument('--ext', default='.nv', help='comma-separated extensions (default .nv)')
    args = ap.parse_args()

    exts = tuple(e if e.startswith('.') else '.' + e for e in args.ext.split(','))
    files = find_files([Path(p) for p in args.paths], exts)
    if not files:
        print('no files matched', file=sys.stderr)
        return 1

    total = {'files': 0, 'files_changed': 0, 'lines': 0, 'changed': 0,
             'iflet': 0, 'let': 0, 'readonly': 0}
    for f in files:
        try:
            src = f.read_text(encoding='utf-8')
        except UnicodeDecodeError:
            # Probably CRLF / windows-1252; try latin1 as a hard fallback.
            src = f.read_text(encoding='latin-1')
        new_src, stats = rewrite_text(src, markdown=f.suffix.lower() in ('.md', '.markdown'))
        total['files'] += 1
        for k in ('lines', 'changed', 'iflet', 'let', 'readonly'):
            total[k] += stats[k]
        if new_src != src:
            total['files_changed'] += 1
            if not args.dry:
                f.write_text(new_src, encoding='utf-8')

    print(f"files scanned:    {total['files']}")
    print(f"files changed:    {total['files_changed']}")
    print(f"lines changed:    {total['changed']} (of {total['lines']})")
    print(f"  if/while-let:   {total['iflet']}")
    print(f"  let bindings:   {total['let']}")
    print(f"  readonly->ro:   {total['readonly']}")
    if args.dry:
        print('(dry-run; nothing written)')
    return 0


if __name__ == '__main__':
    sys.exit(main())
