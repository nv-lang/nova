#!/usr/bin/env python3
"""Fail closed on new Oracle use in Carina build/bootstrap/release paths.

Plan: docs/plans/274-novac-self-hosted-compiler.md (Carina bootstrap policy).

The current REQUIRED novac gate still has recorded Oracle debt. Each legacy
match set is pinned by exact path, pattern, count and SHA256 of its normalized
lines in carina-oracle-debt.json, with a reason and replacement milestone. New
or changed matches fail; stale debt entries fail too, so removing an old Oracle
path requires pruning its row.

This is deliberately NOT wired as a REQUIRED CI check while the known F1 path
still uses the Oracle. It is run by the local commit hook in baseline mode.
When the Carina build/test driver replaces those paths, clear the debt list and
promote this guard to required CI.

The scanner covers active scripts (not selftests), workflows, seed descriptors,
Nova build files/launchers and docs/dev/novac-bootstrap.md. Selftests are not
execution paths; they construct their sabotage strings dynamically.

Usage: python scripts/guards/check-carina-no-oracle-build.py [ROOT]
"""
from __future__ import annotations

import json
import hashlib
import pathlib
import re
import sys
from collections import Counter

NAME = "check-carina-no-oracle-build"
DEBT = "scripts/guards/carina-oracle-debt.json"

PATTERNS = {
    "oracle-package": re.compile(r"\bnova-oracle-[A-Za-z0-9_.-]+", re.I),
    "oracle-version": re.compile(r"\boracle-v[0-9][A-Za-z0-9_.-]*", re.I),
    "oracle-stage-a": re.compile(r"(?:\boracle\b.{0,80}\bstage[- ]?a\b|\bstage[- ]?a\b.{0,80}\boracle\b)", re.I),
    "oracle-selector": re.compile(
        r"\bnovac_find_oracle(?:\s*\(|\s+[\"'$])|\bNOVA_ORACLE_BIN\b|"
        r"\bnovac_oracle_key(?:\s*\(|\s+[\"'$])|"
        r"\b(?:ORACLE|ORACLE_BIN)\s*=\s*[^\n]*nova-cli[/\\]target[/\\]release[/\\]nova(?:\.exe)?",
    ),
    "oracle-cli-call": re.compile(r"\$ORACLE\b.{0,120}\b(?:build|check|emit|test)\b|\bORACLE\s+(?:build|check|emit|test)\b"),
    "oracle-build-novac": re.compile(
        r"(?:\$[A-Za-z_][A-Za-z0-9_]*\"?|[A-Za-z0-9_./\\-]*nova(?:\.exe)?)"
        r"\s+build\b.{0,240}\bnovac(?:[/\\]src[/\\]main\.nv)?",
        re.I,
    ),
    "oracle-run-novac": re.compile(
        r"(?:\$ORACLE|NOVA_ORACLE_BIN)"
        r".{0,160}\b(?:check|emit|test)\b.{0,240}\bnovac(?:[/\\]src)?",
        re.I,
    ),
    "oracle-provenance": re.compile(
        r"(?:compiler_provenance.{0,80}\boracle\b|\boracle\b.{0,80}compiler provenance)", re.I
    ),
}

CODE_SUFFIXES = {".sh", ".py", ".ps1", ".cmd", ".bat", ".yml", ".yaml", ".json", ".toml", ".nv", ".mk", ".cmake"}
SKIP_PARTS = {"selftest", "__pycache__", ".git", "target"}


def scoped_files(root: pathlib.Path) -> list[pathlib.Path]:
    files: set[pathlib.Path] = set()
    roots = (root / "scripts", root / ".github/workflows", root / "novac")
    for base in roots:
        if not base.exists():
            continue
        for path in base.rglob("*"):
            if not path.is_file() or path.suffix.lower() not in CODE_SUFFIXES:
                continue
            rel = path.relative_to(root)
            if any(part in SKIP_PARTS for part in rel.parts):
                continue
            if path.name not in ("Makefile", "CMakeLists.txt") and path.suffix.lower() not in CODE_SUFFIXES:
                continue
            files.add(path)

    seed_dir = root / "docs/dev/carina-seeds"
    if seed_dir.exists():
        files.update(p for p in seed_dir.glob("*.json") if p.is_file())
    bootstrap_doc = root / "docs/dev/novac-bootstrap.md"
    if bootstrap_doc.is_file():
        files.add(bootstrap_doc)
    for base in (root, root / "compiler-codegen", root / "nova-cli", root / "novac", root / "scripts"):
        if base.is_file() and base.name in ("Makefile", "CMakeLists.txt"):
            files.add(base)
        elif base.is_dir():
            files.update(p for p in base.rglob("*") if p.is_file() and p.name in ("Makefile", "CMakeLists.txt")
                         and not any(part in SKIP_PARTS for part in p.relative_to(root).parts))
    return sorted(files)


def active_line(path: pathlib.Path, line: str) -> bool:
    """Skip source comments; prose/bootstrap docs are inspected in full."""
    if path.name == "novac-bootstrap.md":
        return True
    stripped = line.lstrip()
    return not (stripped.startswith("#") or stripped.startswith("//") or stripped.startswith("/*") or stripped.startswith("*") or stripped.startswith("<!--"))


def scan(root: pathlib.Path) -> tuple[Counter[tuple[str, str]], dict[tuple[str, str], list[str]]]:
    hits: Counter[tuple[str, str]] = Counter()
    samples: dict[tuple[str, str], list[str]] = {}
    for path in scoped_files(root):
        rel = path.relative_to(root).as_posix()
        if rel in (DEBT, "scripts/guards/check-carina-no-oracle-build.py"):
            continue
        text = path.read_text(encoding="utf-8", errors="replace").replace("\r", "")
        for line in text.split("\n"):
            sample = line.strip()
            if not sample or not active_line(path, line):
                continue
            for pattern, rx in PATTERNS.items():
                if rx.search(sample):
                    if pattern == "oracle-build-novac":
                        # A prose diagnostic can recommend `nova build novac`;
                        # it is not a compiler selection. Keep only known Oracle
                        # executable paths/variables and F1's selected NOVA_BIN.
                        selected = ("nova-cli/target/release/nova" in sample.lower()
                                    or "$ORACLE" in sample
                                    or "$NOVA_BIN" in sample
                                    or bool(re.match(
                                        r"^(?:&\s*)?(?:[A-Za-z0-9_.:/\\-]*[/\\])?nova(?:\.exe)?[\"']?\s+build\b",
                                        sample, re.I))
                                    or bool(re.match(r"^&\s*\$ORACLE(?:_BIN)?\s+build\b", sample, re.I)))
                        if not selected:
                            continue
                    key = (rel, pattern)
                    hits[key] += 1
                    samples.setdefault(key, []).append(sample)
    return hits, samples


def load_debt(path: pathlib.Path) -> tuple[dict[tuple[str, str], tuple[int, str]], dict[tuple[str, str], tuple[str, str]], list[str]]:
    allowed: dict[tuple[str, str], tuple[int, str]] = {}
    details: dict[tuple[str, str], tuple[str, str]] = {}
    errors: list[str] = []
    if not path.is_file():
        return allowed, details, ["missing debt ledger: " + str(path)]
    try:
        rows = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        return allowed, details, ["cannot read debt ledger: " + str(exc)]
    if not isinstance(rows, list):
        return allowed, details, ["debt ledger must be a JSON array"]
    # Clearing the ledger is the promotion point after all legacy hits have
    # actually disappeared. A missing or malformed file remains fail-closed;
    # an empty array is GREEN only when the scan itself finds no Oracle hits.
    for n, row in enumerate(rows, 1):
        if not isinstance(row, dict) or not all(isinstance(row.get(k), str) and row[k].strip()
                                                for k in ("path", "pattern", "lines_sha256", "reason", "due")):
            errors.append("debt row %d needs path, pattern, lines_sha256, reason and due" % n)
            continue
        key = (row["path"], row["pattern"])
        if row["pattern"] not in PATTERNS:
            errors.append("debt row %d has unknown pattern %s" % (n, row["pattern"]))
            continue
        count = row.get("count")
        if not isinstance(count, int) or count < 1 or not re.fullmatch(r"[0-9a-f]{64}", row["lines_sha256"]):
            errors.append("debt row %d needs a positive count and lowercase SHA256" % n)
            continue
        if key in allowed:
            errors.append("duplicate debt row %d for %s [%s]" % (n, *key))
            continue
        allowed[key] = (count, row["lines_sha256"])
        details[key] = (row["reason"], row["due"])
    return allowed, details, errors


def main(argv: list[str] | None = None) -> int:
    argv = list(sys.argv[1:] if argv is None else argv)
    root = pathlib.Path(argv[0] if argv else ".").resolve()
    if len(argv) > 1:
        print("%s FAIL: usage: %s [ROOT]" % (NAME, pathlib.Path(__file__).name), file=sys.stderr)
        return 2
    debt_path = root / DEBT
    hits, samples = scan(root)
    allowed, details, errors = load_debt(debt_path)
    changed = []
    stale = []
    for key, count in sorted(hits.items()):
        digest = hashlib.sha256("\n".join(sorted(samples[key])).encode("utf-8")).hexdigest()
        expected = allowed.get(key)
        if expected != (count, digest):
            changed.append((key, count, digest, expected))
    for key, (count, digest) in sorted(allowed.items()):
        if key not in hits:
            stale.append((key, count, details[key]))
    if errors or changed or stale:
        print("%s FAIL: unrecorded Oracle use or stale debt" % NAME, file=sys.stderr)
        for error in errors:
            print("  ledger: " + error, file=sys.stderr)
        for (rel, pattern), count, digest, expected in changed:
            state = "NEW/CHANGED" if expected else "NEW"
            print("  %s x%d %s [%s] lines_sha256=%s (baseline=%s)" %
                  (state, count, rel, pattern, digest, expected), file=sys.stderr)
            for sample in sorted(samples[(rel, pattern)])[:3]:
                print("    " + sample[:220], file=sys.stderr)
        for (rel, pattern), count, (reason, due) in stale:
            print("  STALE x%d %s [%s] (reason: %s; due: %s)" %
                  (count, rel, pattern, reason, due), file=sys.stderr)
        print("  Record only existing intentional paths with a reason and replacement milestone in %s." % DEBT,
              file=sys.stderr)
        return 1
    print("%s ok: scoped files=%d legacy Oracle debt matches=%d new matches=0" %
          (NAME, len(scoped_files(root)), sum(hits.values())))
    return 0


if __name__ == "__main__":
    sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
    sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")
    sys.exit(main())
