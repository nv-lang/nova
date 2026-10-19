#!/usr/bin/env python3
"""Build and run one spec-linked native Carina program for an exact golden.

The candidate executable, its digest, the pinned ZIP, and an approved scratch
directory are explicit inputs. No default Oracle CLI or checkout temp path is
accepted.
"""
from __future__ import annotations

import hashlib
import os
import pathlib
import re
import subprocess
import sys
import tempfile

SEED_SHA256 = "7e8264fd7b72d4005ca9bde585a6a732d3e7d6c248b65fd5787cd4a766e69cdb"
B1_SHA256 = "824bf597f8f073c68961df8b824ef5d4e72c4b463c47b727a4e121716f6996f4"
SOURCE_SHA = "3729abcce42ec2d94efc898ec6eafe91ff6210d5"
DRIVER_STAMP = b"NOVA_CARINA_DRIVER_STAMP=carina-b1-build-v1"


def fail(message: str) -> int:
    print("Carina golden FAIL: " + message, file=sys.stderr)
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
    if len(sys.argv) != 2:
        print("usage: carina-build-golden.py <entry.nv>", file=sys.stderr)
        return 2
    root = pathlib.Path(__file__).resolve().parents[2]
    entry = pathlib.Path(sys.argv[1]).resolve()
    if not entry.is_file():
        return fail("entry missing: " + str(entry))

    driver_arg = os.environ.get("NOVA_CARINA_DRIVER")
    driver_sha = os.environ.get("NOVA_CARINA_DRIVER_SHA256", "").lower()
    if not driver_arg or not re.fullmatch(r"[0-9a-f]{64}", driver_sha):
        return fail("NOVA_CARINA_DRIVER and 64-hex NOVA_CARINA_DRIVER_SHA256 are required")
    driver = pathlib.Path(driver_arg).resolve()
    if not driver.is_file():
        return fail("candidate Carina driver missing: " + str(driver))
    actual_driver_sha = file_sha256(driver)
    if actual_driver_sha != driver_sha:
        return fail("candidate driver SHA256 mismatch: " + actual_driver_sha)
    if not contains_stamp(driver):
        return fail("candidate lacks the Carina driver build stamp; refusing to run an Oracle/legacy CLI")

    seed_arg = os.environ.get("NOVA_CARINA_SEED_ZIP")
    if not seed_arg:
        return fail("NOVA_CARINA_SEED_ZIP is required")
    seed_zip = pathlib.Path(seed_arg).resolve()
    if not seed_zip.is_file() or file_sha256(seed_zip) != SEED_SHA256:
        return fail("pinned Carina seed ZIP missing or SHA256 mismatch")

    scratch_arg = os.environ.get("NOVA_TEST_TMPDIR")
    if not scratch_arg:
        return fail("NOVA_TEST_TMPDIR must name an approved scratch directory")
    scratch = pathlib.Path(scratch_arg).resolve()
    if not scratch.is_dir():
        return fail("NOVA_TEST_TMPDIR is not an existing directory: " + str(scratch))
    try:
        scratch.relative_to(root)
    except ValueError:
        pass
    else:
        return fail("NOVA_TEST_TMPDIR must be outside the repository checkout")

    toolchain = os.environ.get("CARINA_GOLDEN_TOOLCHAIN", "clang")
    with tempfile.TemporaryDirectory(prefix="carina-build-golden-", dir=scratch) as tmp:
        exe = pathlib.Path(tmp) / "golden.exe"
        argv = [
            str(driver), "build", str(entry), "-o", str(exe),
            "--mode", "dev", "--toolchain", toolchain, "--timeout", "300",
        ]
        if os.environ.get("CARINA_GOLDEN_CLANG"):
            argv.extend(["--clang", os.environ["CARINA_GOLDEN_CLANG"]])
        if os.environ.get("CARINA_GOLDEN_VCVARS"):
            argv.extend(["--vcvars", os.environ["CARINA_GOLDEN_VCVARS"]])
        build = subprocess.run(argv, cwd=root, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        if build.returncode != 0:
            sys.stderr.buffer.write(build.stdout)
            sys.stderr.buffer.write(build.stderr)
            return build.returncode or 1

        build_stdout = build.stdout.decode("utf-8", "replace")
        build_stderr = build.stderr.decode("utf-8", "replace")
        if "driver_stamp=carina-b1-build-v1" not in build_stdout:
            return fail("build receipt lacks the Carina driver stamp")
        if f"asset_sha256={SEED_SHA256}" not in build_stdout:
            return fail("build receipt lacks the pinned ZIP SHA256")
        if f"b1_sha256={B1_SHA256}" not in build_stdout:
            return fail("build receipt lacks the pinned B1 SHA256")
        if f"source_sha={SOURCE_SHA}" not in build_stdout:
            return fail("build receipt lacks the pinned source SHA")
        if "platform=windows-x86_64" not in build_stdout:
            return fail("build receipt lacks the Windows platform")
        if "seed_freshness=verified" not in build_stdout:
            return fail("build receipt lacks the verified seed freshness marker")
        if not re.search(r"generated_c_sha256=[0-9a-f]{64}\b", build_stdout):
            return fail("build receipt lacks generated-C SHA256")
        if not re.search(r"runtime_sha256=[0-9a-f]{64}\b", build_stdout):
            return fail("build receipt lacks runtime archive SHA256")
        binary_sha = re.search(r"binary_sha256=([0-9a-f]{64})\b", build_stdout)
        if binary_sha is None:
            return fail("build receipt lacks native executable SHA256")
        if f"carina-gate verify-seed PASS: carina-b windows-x86_64 sha256={SEED_SHA256} source={SOURCE_SHA} " not in build_stderr:
            return fail("build stderr lacks the fail-closed pinned seed verifier receipt")
        if not exe.is_file():
            return fail("build succeeded without a native executable")
        if file_sha256(exe) != binary_sha.group(1):
            return fail("native executable SHA256 does not match the build receipt")

        run = subprocess.run([str(exe)], cwd=root, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        sys.stdout.buffer.write(run.stdout)
        sys.stderr.buffer.write(run.stderr)
        return run.returncode


if __name__ == "__main__":
    sys.exit(main())
