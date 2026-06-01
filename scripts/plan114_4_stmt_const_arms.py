#!/usr/bin/env python3
"""Plan 114.4 Ф.2: add `Stmt::Const(_)` match arms next to `Stmt::Let(...)`
in types/mod.rs at specified line numbers.

For each line N, we look at the surrounding `Stmt::Let(...) => { ... }` arm
and add a sibling `Stmt::Const(_) => {}` no-op (or pass-through).
"""
import re
import sys
from pathlib import Path

target = Path('compiler-codegen/src/types/mod.rs')
src = target.read_text(encoding='utf-8')
lines = src.splitlines(keepends=True)

# Find all `Stmt::Let(...)` match arms that don't have a sibling Stmt::Const.
# Walk forward to find `=>` and then the arm body.
# Simple heuristic: after each "Stmt::Let(" pattern, insert sibling arm at
# the same indentation level after the closing brace/expression.

changed = 0
i = 0
out_lines = []
while i < len(lines):
    line = lines[i]
    # Match "Stmt::Let(name) => single_expr," or "Stmt::Let(name) => {"
    m = re.match(r'^(\s+)Stmt::Let\(([^)]*)\)\s*=>\s*(.*)$', line)
    if m:
        indent, _binding, body_start = m.groups()
        # Check if next line is already Stmt::Const
        if i + 1 < len(lines) and 'Stmt::Const(' in lines[i + 1]:
            out_lines.append(line)
            i += 1
            continue
        out_lines.append(line)
        # If body_start ends with `{`, find matching `}` and skip arm.
        if body_start.strip().endswith('{'):
            depth = 1
            j = i + 1
            while j < len(lines) and depth > 0:
                out_lines.append(lines[j])
                depth += lines[j].count('{') - lines[j].count('}')
                j += 1
            # Insert Stmt::Const arm sibling AFTER closing brace.
            out_lines.append(f'{indent}// Plan 114.4 Ф.2: scope-local const — pass-through (no-op for now).\n')
            out_lines.append(f'{indent}Stmt::Const(_) => {{}}\n')
            i = j
            changed += 1
            continue
        else:
            # single-line arm: body_start is the entire expr ending with ','.
            # Insert sibling arm.
            out_lines.append(f'{indent}Stmt::Const(_) => {{}}\n')
            i += 1
            changed += 1
            continue
    out_lines.append(line)
    i += 1

if changed:
    target.write_text(''.join(out_lines), encoding='utf-8')
print(f"sites modified: {changed}")
