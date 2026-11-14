#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""check-carina-gate-ci.py -- the Carina CI contour is oracle-free BY FORM.

Plan: docs/plans/274-novac-self-hosted-compiler.md.
Task 140 (doc/tasks/140-sdelat-ci-kontur-kariny-nezavisimym-ot-o): the
carina-gate workflow builds Carina from the pinned B1 seed and runs the
textual tier of gate-novac.sh without a single Oracle call; the
Oracle-dependent guards live in the advisory carina-oracle-compat workflow.

WHAT IT PROVES (form, not outcome -- the run itself is CI's):

  1. carina-gate.yml exists, carries the bootstrap chain (fetch-seed -> emit
     -> libs -> link -> smoke) and gate-carina.sh, and NO active (non-comment)
     line invokes the Oracle or the Rust toolchain: no cargo, no rustup /
     rust-toolchain, no nova-cli/target path, no novac_find_oracle, no
     $ORACLE / NOVA_BIN selection.
  2. Anti-drift partition: every guard INVOKED in scripts/gate-novac.sh
     (par_add / guard lines) is either in a run list of
     scripts/gate-carina.sh or in exactly one of its EXCLUDED_ORACLE /
     EXCLUDED_CORPUS / EXCLUDED_FULL data blocks, with no overlaps; no
     excluded or run name is a typo (each names a real file in
     scripts/guards/) and no exclusion is stale (each excluded name is
     actually invoked in gate-novac.sh). A new guard added to gate-novac.sh
     without classification reddens this guard.
  3. The move is real, not a drop: every EXCLUDED_ORACLE member is invoked
     (named in an active line) by carina-oracle-compat.yml.
  4. Seed pins agree: SEED_SOURCE / SEED_EXE_SHA256 in
     scripts/tools/carina-native-build.py equal the constants in
     nova-driver/src/carina_build.rs (two truths drift silently otherwise --
     the same class as 274.3/F6).

Selftest: scripts/guards/selftest/test-check-carina-gate-ci.py

$1 -- repository root (default: "."). Exit 1 with every violation, 0 with an
`ok:` line (registry #645: zero without the line is "did not fall", not
"checked").
"""
from __future__ import annotations

import pathlib
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-carina-gate-ci"

CARINA_GATE = ".github/workflows/carina-gate.yml"
ORACLE_COMPAT = ".github/workflows/carina-oracle-compat.yml"
GATE_CARINA = "scripts/gate-carina.sh"
GATE_NOVAC = "scripts/gate-novac.sh"
NATIVE_BUILD = "scripts/tools/carina-native-build.py"
CARINA_BUILD_RS = "nova-driver/src/carina_build.rs"

GUARD_RE = re.compile(r"check-[a-z0-9-]+\.(?:sh|py)")
# Invocation sites in gate-novac.sh: `par_add "<path>" "..."` and
# `guard [--deadline N] "<path>" ...` -- both name scripts/guards/check-*.
INVOKE_RE = re.compile(
    r'^\s*(?:if\b[^\n;]*;\s*then\s+)?(?:par_add|guard)\b[^\n]*scripts/guards/(check-[a-z0-9-]+\.(?:sh|py))')
EXCLUDED_BLOCK_RE = re.compile(r"^(EXCLUDED_(?:ORACLE|CORPUS|FULL))=\"([^\"]*)\"", re.M | re.S)

# Invocation-shaped tokens only: the WORD "oracle" in a step name or a comment
# is legitimate (the workflow says what it is free OF); these are not.
FORBIDDEN_ACTIVE = [
    (re.compile(r"\bcargo\b", re.I), "cargo (Rust toolchain = the Oracle's build)"),
    (re.compile(r"\brustup\b|\brust-toolchain\b|\bdtolnay\b", re.I), "Rust toolchain setup"),
    (re.compile(r"nova-cli[/\\]target", re.I), "the Oracle's build output path"),
    (re.compile(r"\bnovac_find_oracle\b"), "the Oracle selector door"),
    (re.compile(r"\$ORACLE\b|\bNOVA_BIN\b"), "an Oracle binary variable"),
]

REQUIRED_GATE_TOKENS = [
    "carina-native-build.py",
    "fetch-seed",
    "emit",
    "libs",
    "link",
    "smoke",
    "scripts/gate-carina.sh",
]


def active_text(path: pathlib.Path) -> str:
    """File text without comment lines (and without yml trailing comments)."""
    out = []
    for line in path.read_text(encoding="utf-8", errors="replace").replace("\r", "").split("\n"):
        s = line.lstrip()
        if s.startswith("#"):
            continue
        if path.suffix in (".yml", ".yaml"):
            line = line.split(" #")[0]
        out.append(line)
    return "\n".join(out)


def pins(path: pathlib.Path) -> dict[str, str]:
    text = path.read_text(encoding="utf-8", errors="replace")
    out = {}
    for key, pat in (("SEED_SOURCE", r'SEED_SOURCE\s*[:=].{0,24}"([0-9a-f]{40})"'),
                     ("SEED_EXE_SHA256", r'SEED_EXE_SHA256\s*[:=].{0,24}"([0-9a-f]{64})"')):
        m = re.search(pat, text, re.S)
        if m:
            out[key] = m.group(1)
    return out


def main(argv: list[str] | None = None) -> int:
    argv = list(sys.argv[1:] if argv is None else argv)
    root = pathlib.Path(argv[0] if argv else ".").resolve()
    bad: list[str] = []

    paths = {p: root / p for p in (CARINA_GATE, ORACLE_COMPAT, GATE_CARINA, GATE_NOVAC,
                                   NATIVE_BUILD, CARINA_BUILD_RS)}
    for rel, p in paths.items():
        if not p.is_file():
            bad.append("missing file: " + rel)
    if bad:
        for b in bad:
            print(NAME + " FAIL: " + b, file=sys.stderr)
        return 1

    # 1. carina-gate.yml: oracle-free form + the bootstrap chain is visible.
    gate_yml = active_text(paths[CARINA_GATE])
    for token in REQUIRED_GATE_TOKENS:
        if token not in gate_yml:
            bad.append("%s: required token %r absent -- the bootstrap chain or the textual tier is not wired" %
                       (CARINA_GATE, token))
    for rx, what in FORBIDDEN_ACTIVE:
        for line in gate_yml.split("\n"):
            if rx.search(line):
                bad.append("%s: active line invokes %s: %s" % (CARINA_GATE, what, line.strip()[:120]))

    # 2. The partition: invoked in gate-novac.sh == run in gate-carina.sh + excluded.
    novac_text = active_text(paths[GATE_NOVAC])
    invoked = set()
    for line in novac_text.split("\n"):
        m = INVOKE_RE.match(line)
        if m:
            invoked.add(m.group(1))
    if not invoked:
        bad.append("%s: zero guard invocations found -- the target of the partition is lost" % GATE_NOVAC)

    carina_text = active_text(paths[GATE_CARINA])
    excluded: dict[str, str] = {}
    for block, body in EXCLUDED_BLOCK_RE.findall(carina_text):
        for name in GUARD_RE.findall(body):
            if name in excluded:
                bad.append("%s: %s in two exclusion blocks (%s, %s)" % (GATE_CARINA, name, excluded[name], block))
            excluded[name] = block
    run = set(GUARD_RE.findall(carina_text)) - set(excluded)

    for name in sorted(set(excluded) & run):
        bad.append("%s: %s is both run and excluded" % (GATE_CARINA, name))
    for name in sorted(invoked - run - set(excluded)):
        bad.append("%s: %s invoked in gate-novac.sh but neither run nor excluded -- classify it" %
                   (GATE_CARINA, name))
    for name in sorted(set(excluded) - invoked):
        bad.append("%s: stale exclusion %s -- not invoked in gate-novac.sh; remove the row" %
                   (GATE_CARINA, name))
    for name in sorted(run | set(excluded)):
        if not (root / "scripts/guards" / name).is_file():
            bad.append("%s: %s names no file in scripts/guards/" % (GATE_CARINA, name))

    # 3. The oracle-dependent guards are MOVED, not dropped.
    compat_yml = active_text(paths[ORACLE_COMPAT])
    moved = [n for n, block in excluded.items() if block == "EXCLUDED_ORACLE"]
    for name in sorted(moved):
        if name not in compat_yml:
            bad.append("%s: %s is EXCLUDED_ORACLE but not invoked here -- moved out means moved IN here" %
                       (ORACLE_COMPAT, name))

    # 4. Seed pins: one truth in two files.
    py_pins = pins(paths[NATIVE_BUILD])
    rs_pins = pins(paths[CARINA_BUILD_RS])
    for key in ("SEED_SOURCE", "SEED_EXE_SHA256"):
        if key not in py_pins:
            bad.append("%s: no %s pin found" % (NATIVE_BUILD, key))
        if key not in rs_pins:
            bad.append("%s: no %s pin found" % (CARINA_BUILD_RS, key))
        if key in py_pins and key in rs_pins and py_pins[key] != rs_pins[key]:
            bad.append("seed pin %s diverged: %s=%s vs %s=%s" %
                       (key, NATIVE_BUILD, py_pins[key], CARINA_BUILD_RS, rs_pins[key]))

    if bad:
        print("%s FAIL: %d violation(s)" % (NAME, len(bad)), file=sys.stderr)
        for b in bad:
            print("  " + b, file=sys.stderr)
        return 1
    n_moved = len(moved)
    n_corpus = sum(1 for b in excluded.values() if b == "EXCLUDED_CORPUS")
    n_full = sum(1 for b in excluded.values() if b == "EXCLUDED_FULL")
    print("%s ok: carina-gate.yml oracle-free by form; %d guards run, %d moved to oracle-compat, "
          "%d corpus-deferred, %d full-tier; seed pins consistent" %
          (NAME, len(run), n_moved, n_corpus, n_full))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
