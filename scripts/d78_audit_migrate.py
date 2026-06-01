#!/usr/bin/env python3
"""D78 rev-3 audit + migrate.

Scans *.nv files in corpus and:
- AUDIT MODE (--audit): reports current module declaration vs expected
  rev-3 (parent.target), listing all rev-1/legacy violators.
- MIGRATE MODE (--migrate): rewrites legacy declarations to rev-3 form.

D78 rev-3 rule:
    module = <parent_of_target>.<target_name>
    target = file basename (.nv stripped) для single-file;
             folder name для folder-module peer.
    parent = directory сразу над target.

`internal/` special-case (rev-3.1) — `owner.internal.target` (3 segments).

Usage:
    python scripts/d78_audit_migrate.py --audit ROOT [ROOT ...]
    python scripts/d78_audit_migrate.py --migrate ROOT [ROOT ...]
"""
from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

MODULE_RE = re.compile(r'^module\s+([\w\.]+)\s*$', re.MULTILINE)


def expected_rev3_decl(file: Path, root: Path) -> list[str] | None:
    """Compute expected rev-3 module path for `file` rooted at `root`.

    Returns None if not under root.
    """
    try:
        rel = file.relative_to(root).with_suffix('')
    except ValueError:
        return None
    parts = list(rel.parts)
    if not parts:
        return None
    # internal/ special-case rev-3.1
    if 'internal' in parts:
        idx = parts.index('internal')
        owner = parts[idx - 1] if idx > 0 else root.name
        target = parts[-1]
        if target == 'internal':
            return [owner, 'internal']
        return [owner, 'internal', target]
    # single-file: target = filename, parent = parent folder.
    target = parts[-1]
    parent = parts[-2] if len(parts) >= 2 else root.name
    return [parent, target]


def read_module_decl(file: Path) -> str | None:
    try:
        text = file.read_text(encoding='utf-8', errors='replace')
    except OSError:
        return None
    m = MODULE_RE.search(text)
    return m.group(1) if m else None


def audit(roots: list[Path]) -> tuple[int, int, list[tuple[Path, str, str]]]:
    """Returns (compliant_count, violator_count, violators)."""
    compliant = 0
    violators: list[tuple[Path, str, str]] = []
    for root in roots:
        for f in root.rglob('*.nv'):
            decl = read_module_decl(f)
            if decl is None:
                continue
            decl_parts = decl.split('.')
            expected = expected_rev3_decl(f, root)
            if expected is None:
                continue
            if decl_parts == expected:
                compliant += 1
            else:
                violators.append((f, decl, '.'.join(expected)))
    return compliant, len(violators), violators


def migrate(violators: list[tuple[Path, str, str]]) -> int:
    """Rewrite module declarations on disk. Returns count of changed files."""
    changed = 0
    for f, old_decl, new_decl in violators:
        text = f.read_text(encoding='utf-8', errors='replace')
        new_text = re.sub(
            rf'^module\s+{re.escape(old_decl)}\s*$',
            f'module {new_decl}',
            text,
            count=1,
            flags=re.MULTILINE,
        )
        if new_text != text:
            f.write_text(new_text, encoding='utf-8')
            changed += 1
    return changed


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument('roots', nargs='+')
    ap.add_argument('--audit', action='store_true')
    ap.add_argument('--migrate', action='store_true')
    ap.add_argument('--show', type=int, default=20)
    args = ap.parse_args()
    if not (args.audit or args.migrate):
        ap.error('Specify --audit or --migrate')
    roots = [Path(r).resolve() for r in args.roots]
    compliant, violator_count, violators = audit(roots)
    print(f"Compliant (rev-3): {compliant}")
    print(f"Violators (legacy / wrong): {violator_count}")
    if violator_count and not args.migrate:
        print(f"First {min(args.show, violator_count)} violators:")
        for f, decl, expected in violators[:args.show]:
            print(f"  {f}: '{decl}' -> '{expected}'")
    if args.migrate:
        changed = migrate(violators)
        print(f"Migrated {changed} files.")
    return 0


if __name__ == '__main__':
    sys.exit(main())
