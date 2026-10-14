#!/usr/bin/env python3
"""Fail-closed Carina seed and specification-linked conformance helpers.

Descriptors are data supplied by the caller. This tool intentionally does not
discover a live manifest or silently fall back to the Rust Oracle.
"""
import argparse
import hashlib
import json
import os
import pathlib
import re
import subprocess
import sys
import urllib.parse


def fail(message):
    raise ValueError(message)


def load(path):
    with open(path, encoding="utf-8") as f:
        return json.load(f)


def verify_seed(args):
    d = load(args.descriptor)
    required = ("asset_name", "asset_url", "sha256", "platform", "source_revision",
                "compiler_provenance", "stage")
    missing = [k for k in required if not isinstance(d.get(k), str) or not d[k].strip()]
    if missing:
        fail("seed descriptor missing fields: " + ", ".join(missing))
    if not re.fullmatch(r"[0-9a-f]{64}", d["sha256"]):
        fail("seed descriptor SHA256 must be 64 lowercase hex characters")
    u = urllib.parse.urlparse(d["asset_url"])
    if u.scheme != "https" or not u.netloc:
        fail("published asset URL must be HTTPS")
    if pathlib.PurePosixPath(u.path).name != d["asset_name"]:
        fail("published asset name does not match URL")
    if d["platform"] != args.platform:
        fail("wrong platform: descriptor=%s requested=%s" % (d["platform"], args.platform))
    if d["source_revision"] != args.expected_seed_source_revision:
        fail("stale seed: source revision does not match pinned seed revision")
    if not os.path.isfile(args.asset):
        fail("published seed asset is missing: " + args.asset)
    digest = hashlib.sha256(pathlib.Path(args.asset).read_bytes()).hexdigest()
    if digest != d["sha256"]:
        fail("published seed SHA256 mismatch")
    if d["stage"] == "carina-b":
        if not d["compiler_provenance"].startswith("carina:"):
            fail("Carina B compiler provenance must name carina:<revision>")
    elif d["stage"] == "oracle-a":
        if not args.allow_oracle_stage_a or not args.oracle_reason.strip():
            fail("unannounced Oracle fallback: stage oracle-a requires explicit opt-in and reason")
        if not d["compiler_provenance"].startswith("oracle:"):
            fail("Oracle stage A compiler provenance must name oracle:<revision>")
        print("carina-gate: NOTICE — temporary Oracle stage A: " + args.oracle_reason)
    else:
        fail("unsupported seed stage: " + d["stage"])
    print("carina-gate verify-seed PASS: %s %s sha256=%s source=%s compiler=%s" %
          (d["stage"], d["platform"], digest, d["source_revision"], d["compiler_provenance"]))


def run_conformance(args):
    d = load(args.descriptor)
    fixtures = d.get("fixtures")
    if not isinstance(fixtures, list) or not fixtures:
        fail("zero conformance tests: descriptor fixtures must be non-empty")
    if not isinstance(d.get("fixture_count"), int) or d["fixture_count"] != len(fixtures):
        fail("conformance fixture count mismatch: descriptor=%s actual=%d" %
             (d.get("fixture_count"), len(fixtures)))
    command = d.get("command")
    if not isinstance(command, list) or not command or not all(isinstance(x, str) for x in command):
        fail("conformance descriptor requires a command argv list")
    root = pathlib.Path(args.root).resolve()
    passed = 0
    for f in fixtures:
        for k in ("path", "spec", "stdout", "stderr", "exit"):
            if k not in f:
                fail("fixture missing %s" % k)
        if not re.search(r"(?:D\d+|spec/[^\s#]+)", f["spec"]):
            fail("fixture lacks specification link: " + str(f["path"]))
        src = (root / f["path"]).resolve()
        if not src.is_file() or root not in src.parents:
            fail("missing or escaping fixture: " + str(f["path"]))
        text = src.read_text(encoding="utf-8")
        marker = f.get("negative")
        if marker:
            line = marker.get("line")
            code = marker.get("code")
            lines = text.splitlines()
            if not isinstance(line, int) or line < 1 or line > len(lines) or not code:
                fail("invalid line-pinned negative marker: " + f["path"])
            if not re.search(r"nova:expect\s+" + re.escape(code) + r"\b", lines[line - 1]):
                fail("line-pinned negative marker missing at %s:%s" % (f["path"], line))
        argv = [x.replace("{source}", str(src)) for x in command]
        p = subprocess.run(argv, cwd=root, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        out = p.stdout.decode("utf-8", "replace")
        err = p.stderr.decode("utf-8", "replace")
        if (p.returncode, out, err) != (f["exit"], f["stdout"], f["stderr"]):
            fail("golden mismatch: %s (exit=%d expected=%s stdout=%r expected=%r stderr=%r expected=%r)" %
                 (f["path"], p.returncode, f["exit"], out, f["stdout"], err, f["stderr"]))
        passed += 1
    print("carina-gate conformance PASS: %d spec-linked fixtures" % passed)


def main():
    p = argparse.ArgumentParser()
    sub = p.add_subparsers(dest="action", required=True)
    s = sub.add_parser("verify-seed")
    s.add_argument("--descriptor", required=True)
    s.add_argument("--asset", required=True)
    s.add_argument("--platform", required=True)
    s.add_argument("--expected-seed-source-revision", required=True)
    s.add_argument("--allow-oracle-stage-a", action="store_true")
    s.add_argument("--oracle-reason", default="")
    c = sub.add_parser("conformance")
    c.add_argument("--descriptor", required=True)
    c.add_argument("--root", default=".")
    args = p.parse_args()
    try:
        (verify_seed if args.action == "verify-seed" else run_conformance)(args)
    except (OSError, ValueError, json.JSONDecodeError) as e:
        print("carina-gate %s FAIL: %s" % (args.action, e), file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
