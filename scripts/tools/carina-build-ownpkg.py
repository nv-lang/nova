#!/usr/bin/env python3
"""Own-package qualified-import regression check for the pinned Carina B1 driver build.

Registry entry 1924 (task 132): the driver must hand the pinned seed a
RELATIVE edge for a package-qualified import of the program's OWN package
(`import ownpkg_build_min.helper.{greet}`). The seed stages every directory
as one unit but links units only through relative imports, so with the
qualified spelling left in the staged copy the seed builds no entry-to-module
edge and dies with E_NOVAC_ICE "internal compiler error at
novac/src/pipeline/program.nv:157 ... belongs to module ..., which the
entry's imports do not reach" and this script exits RED. The unit tests of
nova-driver/src/program.rs pin the scan/stage/rewrite mechanics but NOT this
wiring, and `nova test` cannot run the fixture through the Oracle (a staged
manifest exists only under target/, or the corpus runner would compile the
files) -- neither replaces this check. The fixture lives under
conformance/fixtures/ because the corpus walker skips any directory named
`fixtures` recursively (D376); see carina-build-dep.py for the placement
lesson (CI run 41108666626).

The FULL stdout of the produced executable is compared line by line
(CRLF-normalized) against the fixture's expected_stdout.txt sidecar: the
greeting comes from the folder-module, the arithmetic from the second
module -- both edges must exist for the program to run.

Inputs (explicit, mirroring carina-build-dep.py):
  NOVA_CARINA_DRIVER          -- release nova-driver binary under test; it
                                 must carry the carina-b1-build-v1 stamp
                                 (an Oracle/legacy CLI is refused).
  CARINA_OWNPKG_TOOLCHAIN     -- native toolchain, default clang.

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

FIXTURE = "ownpkg_build_min"
ENTRY_NAME = "main.nv"


def fail(message: str) -> int:
    print("carina-ownpkg FAIL: " + message, file=sys.stderr)
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
    print("carina-ownpkg: driver=%s sha256=%s" % (driver, file_sha256(driver)))

    src_dir = root / "spec_tests" / "conformance" / "fixtures" / FIXTURE
    entry = src_dir / ENTRY_NAME
    expected_path = src_dir / "expected_stdout.txt"
    if not entry.is_file():
        return fail("fixture entry missing: " + str(entry))
    if not (src_dir / "helper" / "helper.nv").is_file():
        return fail("fixture helper module missing: " + str(src_dir / "helper"))
    if not (src_dir / "math" / "math.nv").is_file():
        return fail("fixture math module missing: " + str(src_dir / "math"))
    if not expected_path.is_file():
        return fail("expected-stdout sidecar missing: " + str(expected_path))
    expected = expected_path.read_text(encoding="utf-8").replace("\r\n", "\n")

    stage = root / "target" / "carina-ownpkg-check" / ("%s.%d" % (FIXTURE, os.getpid()))
    shutil.rmtree(stage, ignore_errors=True)
    stage.mkdir(parents=True)
    for source in sorted(src_dir.rglob("*.nv")):
        target = stage / source.relative_to(src_dir)
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, target)
    # The manifest exists only in the staged copy (target/): the fixture
    # directory itself must stay free of a package manifest, or the Oracle
    # corpus runner would try to compile it (the dep_build_min lesson).
    (stage / "nova.toml").write_text(
        '[package]\nname = "%s"\nversion = "0.1.0"\n\n[lib]\nsrc = "."\n' % FIXTURE,
        encoding="utf-8",
    )

    exe = stage / (FIXTURE + ".exe")
    toolchain = os.environ.get("CARINA_OWNPKG_TOOLCHAIN", "clang")
    argv = [
        str(driver), "build", str(stage / ENTRY_NAME), "-o", str(exe),
        "--mode", "dev", "--toolchain", toolchain, "--timeout", "300",
    ]
    build = subprocess.run(argv, cwd=root, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    build_stdout = build.stdout.decode("utf-8", "replace")
    build_stderr = build.stderr.decode("utf-8", "replace")
    if build.returncode != 0:
        sys.stderr.write(build_stdout)
        sys.stderr.write(build_stderr)
        return fail("driver build failed (exit %d) -- with the pre-132 staging this is the "
                    "E_NOVAC_ICE program.nv:157 red (own-package qualified import, registry 1924)"
                    % build.returncode)
    # The observable of the class: the program's own modules were staged and
    # handed to the seed (a single-file program takes the historical path
    # and prints no such line).
    if "source file(s) staged under" not in build_stdout:
        sys.stderr.write(build_stdout)
        return fail("build receipt lacks the program-staging line; the own-package "
                    "modules were not handed to the seed")
    if "driver_stamp=carina-b1-build-v1" not in build_stdout:
        return fail("build receipt lacks the Carina driver stamp")
    if "asset_sha256=%s" % SEED_SHA256 not in build_stdout:
        return fail("build receipt lacks the pinned seed SHA256")
    if "b1_sha256=%s" % B1_SHA256 not in build_stdout:
        return fail("build receipt lacks the pinned B1 SHA256")
    if "seed_freshness=verified" not in build_stdout:
        return fail("build receipt lacks the verified seed freshness marker")
    binary_sha = re.search(r"binary_sha256=([0-9a-f]{64})\b", build_stdout)
    if binary_sha is None:
        return fail("build receipt lacks the native executable SHA256")
    if not exe.is_file():
        return fail("build succeeded without a native executable")
    if file_sha256(exe) != binary_sha.group(1):
        return fail("native executable SHA256 does not match the build receipt")

    run = subprocess.run([str(exe)], cwd=root, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    if run.returncode != 0:
        sys.stderr.buffer.write(run.stdout)
        sys.stderr.buffer.write(run.stderr)
        return fail("executable exited %d" % run.returncode)
    actual = run.stdout.decode("utf-8", "replace").replace("\r\n", "\n")
    if actual != expected:
        sys.stderr.write("%s: stdout mismatch (full-output check):\n"
                         "--- expected (%d lines) ---\n%s--- actual (%d lines) ---\n%s"
                         % (FIXTURE, len(expected.splitlines()), expected,
                            len(actual.splitlines()), actual))
        return fail("full stdout does not match expected_stdout.txt")

    shutil.rmtree(stage, ignore_errors=True)
    print("carina-ownpkg PASS: %s exit=0 stdout=%d lines binary_sha256=%s"
          % (FIXTURE, len(actual.splitlines()), binary_sha.group(1)))
    print("carina-ownpkg PASS: the own-package qualified-import fixture built through the "
          "pinned Carina B1 seed and matched its full expected stdout")
    return 0


if __name__ == "__main__":
    sys.exit(main())
