#!/usr/bin/env python3
"""External-dependency regression check for the pinned Carina B1 driver build.

Registry entry 1930 (task 129): the driver must hand the pinned seed the
sources of the program's EXTERNAL DEPENDENCIES, staged one directory per
module under `_deps/` with the dependency imports of the staged copies
rewritten to relative paths. With the pre-fix walk (which stopped at the
package boundary) the build below fails with E_NOVAC_SUBSET "no such method
in the declarations handed to novac" and this script exits RED. The unit
tests of nova-driver/src/program.rs pin the scan/stage/rewrite mechanics but
NOT this wiring, and `nova test` cannot run the fixture through the Oracle
(a dependency import does not resolve in the spec_tests package context --
that is why the fixture files carry no EXPECT_* marker) -- neither replaces
this check. The fixture lives under conformance/fixtures/ because the
corpus walker skips any directory named `fixtures` recursively (D376);
the first placement at conformance/dep_build_min/ redened the conformance
gate with E_D78_MODULE_PATH_MISMATCH (CI run 41108666626).

The FULL stdout of the produced executable is compared line by line
(CRLF-normalized) against the fixture's expected_stdout.txt sidecar.

Inputs (explicit, mirroring carina-build-crossmod.py):
  NOVA_CARINA_DRIVER          -- release nova-driver binary under test; it
                                 must carry the carina-b1-build-v1 stamp
                                 (an Oracle/legacy CLI is refused).
  CARINA_DEP_TOOLCHAIN        -- native toolchain, default clang.

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

FIXTURE = "dep_build_min"
ENTRY_NAME = "main.nv"
DEP_NAME = "mathdep"


def fail(message: str) -> int:
    print("carina-dep FAIL: " + message, file=sys.stderr)
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
    print("carina-dep: driver=%s sha256=%s" % (driver, file_sha256(driver)))

    src_dir = root / "spec_tests" / "conformance" / "fixtures" / FIXTURE
    entry = src_dir / ENTRY_NAME
    expected_path = src_dir / "expected_stdout.txt"
    if not entry.is_file():
        return fail("fixture entry missing: " + str(entry))
    if not (src_dir / "dep" / (DEP_NAME + ".nv")).is_file():
        return fail("fixture dependency source missing: " + str(src_dir / "dep"))
    if not expected_path.is_file():
        return fail("expected-stdout sidecar missing: " + str(expected_path))
    expected = expected_path.read_text(encoding="utf-8").replace("\r\n", "\n")

    stage = root / "target" / "carina-dep-check" / ("%s.%d" % (FIXTURE, os.getpid()))
    shutil.rmtree(stage, ignore_errors=True)
    stage.mkdir(parents=True)
    for source in sorted(src_dir.rglob("*.nv")):
        target = stage / source.relative_to(src_dir)
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, target)
    # The consumer's manifest declares the path dependency; the dependency
    # package carries its own. Both manifests exist only in the staged copy
    # (target/): the fixture directory itself must stay importable by NO
    # package manifest, or the Oracle corpus runner would try to compile it.
    (stage / "nova.toml").write_text(
        '[package]\nname = "%s"\nversion = "0.1.0"\n\n[lib]\nsrc = "."\n\n'
        '[dependencies]\n%s = { path = "dep" }\n' % (FIXTURE, DEP_NAME),
        encoding="utf-8",
    )
    (stage / "dep" / "nova.toml").write_text(
        '[package]\nname = "%s"\nversion = "0.1.0"\n\n[lib]\nsrc = "."\n' % DEP_NAME,
        encoding="utf-8",
    )

    exe = stage / (FIXTURE + ".exe")
    toolchain = os.environ.get("CARINA_DEP_TOOLCHAIN", "clang")
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
        return fail("driver build failed (exit %d) -- with the pre-129 walk this is the "
                    'E_NOVAC_SUBSET "no such method in the declarations handed to novac" red'
                    % build.returncode)
    # The observable of the class: the dependency's sources were staged and
    # handed to the seed (a program with no reachable files takes the
    # historical single-file path and prints no such line).
    if "source file(s) staged under" not in build_stdout:
        sys.stderr.write(build_stdout)
        return fail("build receipt lacks the program-staging line; the dependency "
                    "sources were not handed to the seed")
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
    print("carina-dep PASS: %s exit=0 stdout=%d lines binary_sha256=%s"
          % (FIXTURE, len(actual.splitlines()), binary_sha.group(1)))
    print("carina-dep PASS: the dependency fixture built through the pinned Carina B1 "
          "seed and matched its full expected stdout")
    return 0


if __name__ == "__main__":
    sys.exit(main())
