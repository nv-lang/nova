#!/usr/bin/env python3
"""Verify the published Windows Carina B1 emits the pinned stage-A C bytes."""

from __future__ import annotations

import argparse
import hashlib
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import zipfile


SOURCE_REVISION = "3729abcce42ec2d94efc898ec6eafe91ff6210d5"
ASSET_SHA256 = "7e8264fd7b72d4005ca9bde585a6a732d3e7d6c248b65fd5787cd4a766e69cdb"
NOVAC_SHA256 = "824bf597f8f073c68961df8b824ef5d4e72c4b463c47b727a4e121716f6996f4"
EXPECTED_C_SIZE = 13_250_302
EXPECTED_C_SHA256 = "25c5404f1e8e175652f19c9ca6cb03d29de82cee70a7cd9f4258c6724115c8a0"


class VerificationError(Exception):
    pass


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def check_source_tree(source: Path) -> None:
    ancestor = subprocess.run(
        ["git", "-C", str(source), "merge-base", "--is-ancestor", SOURCE_REVISION, "HEAD"],
        capture_output=True, check=False,
    )
    unchanged = subprocess.run(
        ["git", "-C", str(source), "diff", "--quiet", SOURCE_REVISION, "--", "novac/src", "std/src"],
        capture_output=True, check=False,
    )
    status = subprocess.run(
        ["git", "-C", str(source), "status", "--porcelain", "--", "novac/src", "std/src"],
        capture_output=True, text=True, check=False,
    )
    if ancestor.returncode != 0 or unchanged.returncode != 0 or status.returncode != 0 or status.stdout.strip():
        raise VerificationError("RED: source tree revision does not match pinned source")


def validate_asset(asset: Path) -> tuple[bytes, str | None]:
    if not asset.is_file():
        raise VerificationError("RED: asset input is not a file")
    if asset.suffix.lower() == ".zip":
        raw = asset.read_bytes()
        if sha256(raw) != ASSET_SHA256:
            raise VerificationError("RED: published ZIP SHA-256 mismatch")
        try:
            with zipfile.ZipFile(asset) as archive:
                names = [n for n in archive.namelist() if Path(n).name.lower() == "novac.exe"]
                if len(names) != 1:
                    raise VerificationError("RED: ZIP must contain exactly one novac.exe")
                exe = archive.read(names[0])
                for member in archive.infolist():
                    target = Path(member.filename)
                    if target.is_absolute() or ".." in target.parts:
                        raise VerificationError("RED: ZIP contains an unsafe path")
        except (OSError, zipfile.BadZipFile, KeyError) as exc:
            raise VerificationError("RED: cannot read published ZIP") from exc
        relative_exe = names[0]
    else:
        exe = asset.read_bytes()
        relative_exe = None
    if sha256(exe) != NOVAC_SHA256:
        raise VerificationError("RED: published B1 novac.exe SHA-256 mismatch")
    return exe, relative_exe


def check_c(data: bytes, expected_size: int = EXPECTED_C_SIZE,
            expected_sha256: str = EXPECTED_C_SHA256) -> None:
    if len(data) != expected_size or sha256(data) != expected_sha256:
        raise VerificationError("RED: emitted C does not match pinned stage-A C digest")


def verify(source: Path, asset: Path, work_dir: Path) -> None:
    if os.name != "nt":
        raise VerificationError("RED: published Windows B1 can only be executed on Windows")
    if not source.is_dir() or not work_dir.is_dir():
        raise VerificationError("RED: source-tree and existing scratchpad work-dir are required")
    check_source_tree(source)
    _, relative_exe = validate_asset(asset)
    asset_sha = sha256(asset.read_bytes())
    with tempfile.TemporaryDirectory(prefix="carina-c-level-", dir=work_dir) as scratch:
        scratch_dir = Path(scratch)
        if relative_exe is not None:
            with zipfile.ZipFile(asset) as archive:
                archive.extractall(scratch_dir)
            exe = scratch_dir / Path(relative_exe)
        else:
            exe = scratch_dir / "novac.exe"
            exe.write_bytes(asset.read_bytes())
        env = {key: value for key, value in os.environ.items()
               if not key.upper().startswith("NOVA_") and key.upper() != "NOVAC_SELF_PATH"}
        env["NOVA_STD_PATH"] = str(source / "std" / "src")
        env["NOVAC_SELF_PATH"] = "novac/src"
        result = subprocess.run(
            [str(exe), "emit", "novac/src/main.nv"],
            cwd=source,
            env=env,
            capture_output=True,
            check=False,
        )
        if result.returncode != 0:
            raise VerificationError("RED: B1 emit exited nonzero")
        if result.stderr:
            raise VerificationError("RED: B1 emit stderr is not empty")
        check_c(result.stdout)
        (scratch_dir / "emitted.c").write_bytes(result.stdout)
    print(f"PASS: Windows B1 emit exit=0; stage-A C: {EXPECTED_C_SIZE} bytes SHA-256 {EXPECTED_C_SHA256}; stderr empty")
    print(f"PASS: source content {SOURCE_REVISION}; input asset SHA-256 {asset_sha}; B1 SHA-256 {NOVAC_SHA256}")
    print("PROVENANCE: stage A Oracle 0.1.0; GC headers Oracle 0.1.0; saved run2 libraries/source3729; Oracle not executed")
    print("SCOPE: Windows C-level self-emit only; no native EXE/PDB/LIB equality or Linux claim")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source-tree", type=Path, required=True)
    parser.add_argument("--asset", type=Path, required=True,
                        help="published ZIP or its shipped novac.exe")
    parser.add_argument("--work-dir", type=Path, required=True,
                        help="existing session scratchpad directory")
    args = parser.parse_args()
    try:
        verify(args.source_tree.resolve(), args.asset.resolve(), args.work_dir.resolve())
    except VerificationError as exc:
        print(str(exc), file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
