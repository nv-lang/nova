#!/usr/bin/env python3
"""Selftest carina-gate using temporary fixture descriptors only."""
import hashlib
import json
import pathlib
import subprocess
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[3]
TOOL = ROOT / "scripts/tools/carina-gate.py"


def call(*args):
    return subprocess.run([sys.executable, str(TOOL), *map(str, args)],
                          stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)


def main():
    with tempfile.TemporaryDirectory(prefix="carina-gate-selftest-") as td:
        t = pathlib.Path(td)
        asset = t / "nova-carina.zip"
        asset.write_bytes(b"fixture asset bytes")
        sha = hashlib.sha256(asset.read_bytes()).hexdigest()
        desc = {"asset_name": asset.name, "asset_url": "https://example.invalid/releases/" + asset.name,
                "sha256": sha, "platform": "windows-x86_64", "source_revision": "abc123",
                "compiler_provenance": "carina:def456", "stage": "carina-b"}
        df = t / "seed.json"

        def seed(d=desc, platform="windows-x86_64", extra=()):
            df.write_text(json.dumps(d), encoding="utf-8")
            return call("verify-seed", "--descriptor", df, "--asset", asset,
                        "--platform", platform, "--expected-seed-source-revision", "abc123", *extra)

        assert seed().returncode == 0, seed().stderr
        for case, change, platform, extra in [
            ("missing SHA", {k: v for k, v in desc.items() if k != "sha256"}, "windows-x86_64", ()),
            ("mismatched SHA", {**desc, "sha256": "0" * 64}, "windows-x86_64", ()),
            ("wrong platform", desc, "linux-x86_64", ()),
            ("stale seed", {**desc, "source_revision": "old"}, "windows-x86_64", ()),
            ("unannounced Oracle fallback", {**desc, "stage": "oracle-a", "compiler_provenance": "oracle:abc"}, "windows-x86_64", ()),
        ]:
            p = seed(change, platform, extra)
            assert p.returncode != 0 and "FAIL" in p.stderr, (case, p.stdout, p.stderr)
        oracle = {**desc, "stage": "oracle-a", "compiler_provenance": "oracle:abc",
                  "platform": "linux-x86_64"}
        p = seed(oracle, "linux-x86_64")
        assert p.returncode != 0 and "unannounced Oracle fallback" in p.stderr
        p = seed(oracle, "linux-x86_64", ("--allow-oracle-stage-a", "--oracle-reason", "temporary stage A"))
        assert p.returncode == 0 and "temporary Oracle stage A" in p.stdout
        # A correctly described published Carina B is accepted on Windows.
        assert seed(desc).returncode == 0

        fixture = t / "positive.nv"
        fixture.write_text("// spec: D1\nfn main() {}\n", encoding="utf-8")
        negative = t / "negative.nv"
        negative.write_text("// spec: D2\n// nova:expect E_FIXTURE @2\n", encoding="utf-8")
        conformance = {"fixture_count": 2, "command": [sys.executable, "-c", "import sys; sys.stdout.buffer.write(b'ok\\n')", "{source}"], "fixtures": [
            {"path": "positive.nv", "spec": "D1", "stdout": "ok\n", "stderr": "", "exit": 0},
            {"path": "negative.nv", "spec": "D2", "stdout": "ok\n", "stderr": "", "exit": 0,
             "negative": {"code": "E_FIXTURE", "line": 2}},
        ]}
        cf = t / "conformance.json"
        cf.write_text(json.dumps(conformance), encoding="utf-8")
        p = call("conformance", "--descriptor", cf, "--root", t)
        assert p.returncode == 0, p.stderr
        broken = json.loads(json.dumps(conformance))
        broken["fixtures"][0]["stdout"] = "sabotaged\n"
        cf.write_text(json.dumps(broken), encoding="utf-8")
        p = call("conformance", "--descriptor", cf, "--root", t)
        assert p.returncode != 0 and "golden mismatch" in p.stderr
        print("PROBE golden sabotage RED: " + p.stderr.strip())
        cf.write_text(json.dumps(conformance), encoding="utf-8")
        p = call("conformance", "--descriptor", cf, "--root", t)
        assert p.returncode == 0, p.stderr
        print("PROBE golden restored GREEN: " + p.stdout.strip())
        conformance["fixtures"].pop(0)
        cf.write_text(json.dumps(conformance), encoding="utf-8")
        p = call("conformance", "--descriptor", cf, "--root", t)
        assert p.returncode != 0 and "fixture count mismatch" in p.stderr
        print("PROBE fixture removed RED: " + p.stderr.strip())
        conformance["fixtures"].insert(0, broken["fixtures"][0])
        conformance["fixtures"][0]["stdout"] = "ok\n"
        cf.write_text(json.dumps(conformance), encoding="utf-8")
        p = call("conformance", "--descriptor", cf, "--root", t)
        assert p.returncode == 0, p.stderr
        print("PROBE fixture restored GREEN: " + p.stdout.strip())
        conformance["fixtures"] = []
        conformance["fixture_count"] = 0
        cf.write_text(json.dumps(conformance), encoding="utf-8")
        p = call("conformance", "--descriptor", cf, "--root", t)
        assert p.returncode != 0 and "zero conformance tests" in p.stderr
        conformance["fixture_count"] = 1
        conformance["fixtures"] = [{"path": "negative.nv", "spec": "D2", "stdout": "ok\n",
                                    "stderr": "", "exit": 0,
                                    "negative": {"code": "E_FIXTURE", "line": 1}}]
        cf.write_text(json.dumps(conformance), encoding="utf-8")
        p = call("conformance", "--descriptor", cf, "--root", t)
        assert p.returncode != 0 and "line-pinned" in p.stderr
    print("test-carina-gate.py PASS: seed descriptor and spec-golden sabotage probes")


if __name__ == "__main__":
    main()
