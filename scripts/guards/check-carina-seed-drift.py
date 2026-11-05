#!/usr/bin/env python3
"""The drift guard of the Carina seed contour (owner decision 2026-11-04,
doc/tasks/140-sdelat-ci-kontur-kariny-nezavisimym-ot-orakula/owner-decisions.md
section 4, mechanism 2).

The CI contour compiles the PINNED tree (the seed descriptor's
source_revision) because the published seed speaks the language snapshot of
its release. The live tree drifts past it; the drift is legitimate only as
named work. This guard PUBLISHES the drift as a list (every file of
novac/src and std/src that differs between the seed's source revision and
HEAD) and holds it on a ratchet:

  * a drifted file not in the baseline is RED (drift without a task);
  * a baseline row must carry a task number (`# task NNN`);
  * a baseline row whose file no longer drifts is STALE and RED (the
    baseline is regenerated when the seed re-pins, not left to rot).

Fail-closed: no descriptor, no git, no such revision -- all RED, never
"nothing to judge".
"""
from __future__ import annotations

import json
import pathlib
import re
import subprocess
import sys

NAME = "check-carina-seed-drift"
DESCRIPTOR = "docs/dev/carina-seeds/windows-x86_64.json"
BASELINE = "scripts/guards/carina-seed-drift.baseline"
SCOPES = ("novac/src", "std/src")
ROW_RE = re.compile(r"^(\S+) # task (\d+)( .*)?$")


def fail(msg: str) -> int:
    print("%s FAIL: %s" % (NAME, msg), file=sys.stderr)
    return 1


def seed_revision(root: pathlib.Path) -> str | None:
    path = root / DESCRIPTOR
    if not path.is_file():
        return None
    try:
        import json as _json
        rev = _json.loads(path.read_text(encoding="utf-8"))["source_revision"]
    except (OSError, ValueError, KeyError):
        return None
    return rev if re.fullmatch(r"[0-9a-f]{40}", rev) else None


def drift(root: pathlib.Path, rev: str) -> list[str] | None:
    try:
        proc = subprocess.run(
            ["git", "-C", str(root), "diff", "--name-only", rev, "HEAD", "--"] + list(SCOPES),
            capture_output=True, timeout=60)
    except (OSError, subprocess.TimeoutExpired):
        return None
    if proc.returncode != 0:
        return None
    return sorted(p for p in proc.stdout.decode("utf-8", "replace").splitlines() if p.strip())


def load_baseline(root: pathlib.Path) -> tuple[dict[str, str], list[str]]:
    """path -> task number; errors as strings."""
    rows: dict[str, str] = {}
    errors: list[str] = []
    path = root / BASELINE
    if not path.is_file():
        return rows, ["no baseline file: " + BASELINE]
    for n, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        m = ROW_RE.match(line)
        if not m:
            errors.append("baseline row %d does not match '<path> # task NNN': %s" % (n, line[:120]))
            continue
        if m.group(1) in rows:
            errors.append("baseline row %d duplicates %s" % (n, m.group(1)))
            continue
        rows[m.group(1)] = m.group(2)
    return rows, errors


def main(argv: list[str] | None = None) -> int:
    argv = list(sys.argv[1:] if argv is None else argv)
    root = pathlib.Path(argv[0] if argv else ".").resolve()
    rev = seed_revision(root)
    if rev is None:
        return fail("no readable seed descriptor %s with a full source_revision -- "
                    "the pinned world is undefined" % DESCRIPTOR)
    files = drift(root, rev)
    if files is None:
        return fail("git diff %s..HEAD over %s failed (shallow clone? no such revision?) -- "
                    "drift cannot be measured, and unmeasured drift is not green" % (rev, "/".join(SCOPES)))
    baseline, errors = load_baseline(root)
    bad = list(errors)
    for f in files:
        if f not in baseline:
            bad.append("drift without a task: %s (differs from the seed's tree, no baseline row)" % f)
    for f in sorted(baseline):
        if f not in files:
            bad.append("stale baseline row: %s no longer drifts (re-pin regenerates, not rots)" % f)
    if bad:
        print("%s FAIL: drift vs seed %s (%s..HEAD over %s):" % (NAME, DESCRIPTOR, rev[:12], "/".join(SCOPES)),
              file=sys.stderr)
        for b in bad:
            print("  " + b, file=sys.stderr)
        return 1
    # The published list (owner decision: the drift is published, not hidden).
    print("%s: drift of HEAD vs seed %s over %s -- %d file(s), all with tasks:"
          % (NAME, rev[:12], "/".join(SCOPES), len(files)))
    for f in files:
        print("  %s # task %s" % (f, baseline[f]))
    print("%s ok: %d drifted file(s), each named with its task; no growth without a number"
          % (NAME, len(files)))
    return 0


if __name__ == "__main__":
    sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
    sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")
    sys.exit(main(sys.argv[1:]))
