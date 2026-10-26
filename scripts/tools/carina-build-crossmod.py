#!/usr/bin/env python3
"""Multi-module regression check for the pinned Carina B1 driver build.

Registry entry 1889: the driver must hand the pinned seed the PROGRAM's own
module graph (staged under target/carina-build/), not the compiler's own
novac/src tree. With the pre-fix call contract (NOVAC_SELF_PATH = novac/src)
every build below fails with E_NOVAC_SUBSET and this script exits RED. The
unit tests of nova-driver/src/program.rs pin the scan/staging mechanics but
NOT this wiring, and `nova test` runs these fixtures through the Oracle, not
through the driver + pinned seed path -- neither replaces this check.

For each fixture the FULL stdout of the produced executable is compared line
by line (CRLF-normalized) against the fixture's expected_stdout.txt sidecar:
the EXPECT_STDOUT marker is a substring check and would not notice a
truncated or reordered program run.

Inputs (explicit, mirroring carina-build-golden.py):
  NOVA_CARINA_DRIVER          -- release nova-driver binary under test; it
                                 must carry the carina-b1-build-v1 stamp
                                 (an Oracle/legacy CLI is refused).
  CARINA_CROSSMOD_TOOLCHAIN   -- native toolchain, default clang.

Staged package copies live under the repository's target/ (gitignored):
find_workspace_root requires the entry to sit inside a Nova workspace tree.
"""
from __future__ import annotations

import hashlib
import os
import pathlib
import re
import shutil
import subprocess
import sys

SEED_SHA256 = "7e8264fd7b72d4005ca9bde585a6a732d3e7d6c248b65fd5787cd4a766e69cdb"
B1_SHA256 = "824bf597f8f073c68961df8b824ef5d4e72c4b463c47b727a4e121716f6996f4"
DRIVER_STAMP = b"NOVA_CARINA_DRIVER_STAMP=carina-b1-build-v1"

FIXTURES = [
    ("crossmod_build_min", "main.nv"),
    ("crossmod_build_program", "prog.nv"),
]


def fail(message: str) -> int:
    print("carina-crossmod FAIL: " + message, file=sys.stderr)
    return 1


def file_sha256(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def contains_stamp(path: pathlib.Path) -> bool:
    overlap = b""
    with path.open("rb") as source:
        while chunk := source.read(1024 * 1024):
            data = overlap + chunk
            if DRIVER_STAMP in data:
                return True
            overlap = data[-(len(DRIVER_STAMP) - 1):]
    return False


def run_fixture(root: pathlib.Path, driver: pathlib.Path, name: str, entry_name: str,
                toolchain: str) -> int:
    src_dir = root / "spec_tests" / "conformance" / name
    entry = src_dir / entry_name
    expected_path = src_dir / "expected_stdout.txt"
    if not entry.is_file():
        return fail("fixture entry missing: " + str(entry))
    if not expected_path.is_file():
        return fail("expected-stdout sidecar missing: " + str(expected_path))
    expected = expected_path.read_text(encoding="utf-8").replace("\r\n", "\n")

    stage = root / "target" / "carina-crossmod-check" / ("%s.%d" % (name, os.getpid()))
    shutil.rmtree(stage, ignore_errors=True)
    stage.mkdir(parents=True)
    for source in sorted(src_dir.rglob("*.nv")):
        target = stage / source.relative_to(src_dir)
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, target)
    (stage / "nova.toml").write_text(
        '[package]\nname = "%s"\n\n[lib]\nsrc = "."\n' % name, encoding="utf-8")

    exe = stage / (name + ".exe")
    argv = [
        str(driver), "build", str(stage / entry_name), "-o", str(exe),
        "--mode", "dev", "--toolchain", toolchain, "--timeout", "300",
    ]
    build = subprocess.run(argv, cwd=root, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    build_stdout = build.stdout.decode("utf-8", "replace")
    build_stderr = build.stderr.decode("utf-8", "replace")
    if build.returncode != 0:
        sys.stderr.write(build_stdout)
        sys.stderr.write(build_stderr)
        return fail("%s: driver build failed (exit %d) -- with the pre-1889 call contract "
                    "this is the E_NOVAC_SUBSET red" % (name, build.returncode))
    if "driver_stamp=carina-b1-build-v1" not in build_stdout:
        return fail("%s: build receipt lacks the Carina driver stamp" % name)
    if "asset_sha256=%s" % SEED_SHA256 not in build_stdout:
        return fail("%s: build receipt lacks the pinned seed SHA256" % name)
    if "b1_sha256=%s" % B1_SHA256 not in build_stdout:
        return fail("%s: build receipt lacks the pinned B1 SHA256" % name)
    if "seed_freshness=verified" not in build_stdout:
        return fail("%s: build receipt lacks the verified seed freshness marker" % name)
    binary_sha = re.search(r"binary_sha256=([0-9a-f]{64})\b", build_stdout)
    if binary_sha is None:
        return fail("%s: build receipt lacks the native executable SHA256" % name)
    if not exe.is_file():
        return fail("%s: build succeeded without a native executable" % name)
    if file_sha256(exe) != binary_sha.group(1):
        return fail("%s: native executable SHA256 does not match the build receipt" % name)

    run = subprocess.run([str(exe)], cwd=root, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    if run.returncode != 0:
        sys.stderr.buffer.write(run.stdout)
        sys.stderr.buffer.write(run.stderr)
        return fail("%s: executable exited %d" % (name, run.returncode))
    actual = run.stdout.decode("utf-8", "replace").replace("\r\n", "\n")
    if actual != expected:
        sys.stderr.write("%s: stdout mismatch (full-output check, marker is substring-only):\n"
                         "--- expected (%d lines) ---\n%s--- actual (%d lines) ---\n%s"
                         % (name, len(expected.splitlines()), expected,
                            len(actual.splitlines()), actual))
        return fail("%s: full stdout does not match expected_stdout.txt" % name)

    shutil.rmtree(stage, ignore_errors=True)
    print("carina-crossmod PASS: %s exit=0 stdout=%d lines binary_sha256=%s"
          % (name, len(actual.splitlines()), binary_sha.group(1)))
    return 0


def main() -> int:
    root = pathlib.Path(__file__).resolve().parents[2]
    driver_arg = os.environ.get("NOVA_CARINA_DRIVER")
    if not driver_arg:
        return fail("NOVA_CARINA_DRIVER must name the release nova-driver binary under test")
    driver = pathlib.Path(driver_arg).resolve()
    if not driver.is_file():
        return fail("driver missing: " + str(driver))
    if not contains_stamp(driver):
        return fail("driver lacks the carina-b1-build-v1 stamp; refusing an Oracle/legacy CLI")
    print("carina-crossmod: driver=%s sha256=%s" % (driver, file_sha256(driver)))

    toolchain = os.environ.get("CARINA_CROSSMOD_TOOLCHAIN", "clang")
    for name, entry_name in FIXTURES:
        rc = run_fixture(root, driver, name, entry_name, toolchain)
        if rc != 0:
            return rc
    print("carina-crossmod PASS: all %d multi-module fixtures built through the pinned "
          "Carina B1 seed and matched their full expected stdout" % len(FIXTURES))
    return 0


if __name__ == "__main__":
    sys.exit(main())
