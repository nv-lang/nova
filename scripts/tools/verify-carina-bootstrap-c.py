#!/usr/bin/env python3
"""Verify the published Windows Carina B1 emits the pinned stage-A C bytes."""

from __future__ import annotations

import argparse
import hashlib
from io import BytesIO
import json
import os
from pathlib import Path, PurePosixPath, PureWindowsPath
import subprocess
import sys
import tempfile
import zipfile


BASELINE = Path(__file__).resolve().parents[1] / "guards" / "carina-bootstrap-c.baseline"


class VerificationError(Exception):
    pass


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def load_baseline(path: Path = BASELINE) -> dict:
    try:
        baseline = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as exc:
        raise VerificationError("RED: managed Carina C baseline is missing or invalid") from exc
    required = {
        "schema", "platform", "asset_name", "asset_url", "asset_sha256",
        "executable_member", "b1_exe_sha256", "source_revision", "source_paths", "self_path",
        "std_path", "workspace_manifest", "nested_novac_manifest",
        "expected_c_bytes", "expected_c_sha256", "provenance",
        "update_reason",
    }
    if not isinstance(baseline, dict) or not required.issubset(baseline):
        raise VerificationError("RED: managed Carina C baseline schema is incomplete")
    if (baseline["schema"] != 1 or not isinstance(baseline["update_reason"], str)
            or len(baseline["update_reason"].strip()) < 30):
        raise VerificationError("RED: managed Carina C baseline schema/reason invalid")
    for key in ("asset_sha256", "b1_exe_sha256", "source_revision", "expected_c_sha256"):
        value = baseline[key]
        if not isinstance(value, str) or len(value) != 40 and key == "source_revision" or len(value) != 64 and key != "source_revision":
            raise VerificationError("RED: managed Carina C baseline digest malformed")
        try:
            int(value, 16)
        except ValueError as exc:
            raise VerificationError("RED: managed Carina C baseline digest malformed") from exc
    if not isinstance(baseline["expected_c_bytes"], int) or baseline["expected_c_bytes"] <= 0:
        raise VerificationError("RED: managed Carina C baseline size invalid")
    if (not isinstance(baseline["source_paths"], list) or not baseline["source_paths"]
            or any(not isinstance(path, str) or path.startswith("/") or ".." in PurePosixPath(path).parts
                   for path in baseline["source_paths"])):
        raise VerificationError("RED: managed Carina C baseline source paths invalid")
    self_path = baseline["self_path"]
    if not isinstance(self_path, str) or PurePosixPath(self_path).is_absolute() or ".." in PurePosixPath(self_path).parts:
        raise VerificationError("RED: managed Carina C baseline self path must be relative")
    if not isinstance(baseline["provenance"], dict) or not baseline["provenance"]:
        raise VerificationError("RED: managed Carina C baseline provenance invalid")
    return baseline


def check_source_tree(source: Path, baseline: dict) -> None:
    source_revision = baseline["source_revision"]
    ancestor = subprocess.run(
        ["git", "-C", str(source), "merge-base", "--is-ancestor", source_revision, "HEAD"],
        capture_output=True, check=False,
    )
    unchanged = subprocess.run(
        ["git", "-C", str(source), "diff", "--quiet", source_revision, "--", *baseline["source_paths"]],
        capture_output=True, check=False,
    )
    status = subprocess.run(
        ["git", "-C", str(source), "status", "--porcelain", "--untracked-files=all", "--ignored=matching",
         "--", *baseline["source_paths"]],
        capture_output=True, text=True, check=False,
    )
    validate_source_state(ancestor.returncode, unchanged.returncode, status.returncode, status.stdout)
    try:
        root_manifest = (source / baseline["workspace_manifest"]).read_text(encoding="utf-8")
        nested_manifest = (source / baseline["nested_novac_manifest"]).read_text(encoding="utf-8")
    except (OSError, UnicodeError) as exc:
        raise VerificationError("RED: source tree workspace manifests are missing or unreadable") from exc
    validate_workspace_markers("[workspace]" in root_manifest, "[workspace]" in nested_manifest)


def validate_source_state(ancestor_rc: int, diff_rc: int, status_rc: int, status_text: str) -> None:
    if ancestor_rc != 0 or diff_rc != 0 or status_rc != 0 or status_text.strip():
        raise VerificationError("RED: source tree does not match managed source revision")


def validate_workspace_markers(root_has_workspace: bool, nested_has_workspace: bool) -> None:
    if not root_has_workspace or nested_has_workspace:
        raise VerificationError("RED: source tree project-root workspace markers differ from pinned layout")


def validate_zip_data(asset_name: str, raw: bytes, baseline: dict) -> tuple[bytes, str]:
    if asset_name != baseline["asset_name"]:
        raise VerificationError("RED: ZIP name does not match managed release asset")
    if sha256(raw) != baseline["asset_sha256"]:
        raise VerificationError("RED: published ZIP SHA-256 mismatch")
    try:
        with zipfile.ZipFile(BytesIO(raw)) as archive:
            members = archive.namelist()
            member = baseline["executable_member"]
            if members.count(member) != 1:
                raise VerificationError("RED: ZIP must contain exactly one pinned executable member")
            exe = archive.read(member)
            for member in archive.infolist():
                posix = PurePosixPath(member.filename)
                windows = PureWindowsPath(member.filename)
                if posix.is_absolute() or windows.is_absolute() or ".." in posix.parts or ".." in windows.parts:
                    raise VerificationError("RED: ZIP contains an unsafe path")
    except (OSError, zipfile.BadZipFile, KeyError) as exc:
        raise VerificationError("RED: cannot read published ZIP") from exc
    validate_b1_executable(exe, baseline)
    return exe, baseline["executable_member"]


def validate_b1_executable(exe: bytes, baseline: dict) -> bytes:
    if sha256(exe) != baseline["b1_exe_sha256"]:
        raise VerificationError("RED: published B1 executable SHA-256 mismatch")
    return exe


def validate_asset(asset: Path, baseline: dict) -> tuple[bytes, str | None]:
    if not asset.is_file():
        raise VerificationError("RED: asset input is not a file")
    if asset.suffix.lower() == ".zip":
        exe, relative_exe = validate_zip_data(asset.name, asset.read_bytes(), baseline)
    else:
        exe = validate_b1_executable(asset.read_bytes(), baseline)
        relative_exe = None
    return exe, relative_exe


def check_c(data: bytes, baseline: dict) -> None:
    if len(data) != baseline["expected_c_bytes"] or sha256(data) != baseline["expected_c_sha256"]:
        raise VerificationError("RED: emitted C does not match pinned stage-A C digest")


def validate_c_observation(data: bytes, baseline: dict) -> str:
    check_c(data, baseline)
    return f"GREEN: {len(data)} bytes SHA-256 {sha256(data)}"


def verify(source: Path, asset: Path, work_dir: Path) -> None:
    if os.name != "nt":
        raise VerificationError("RED: published Windows B1 can only be executed on Windows")
    if not source.is_dir() or not work_dir.is_dir():
        raise VerificationError("RED: source-tree and existing scratchpad work-dir are required")
    baseline = load_baseline()
    check_source_tree(source, baseline)
    _, relative_exe = validate_asset(asset, baseline)
    asset_sha = sha256(asset.read_bytes())
    with tempfile.TemporaryDirectory(prefix="carina-c-level-", dir=work_dir) as scratch:
        scratch_dir = Path(scratch)
        if relative_exe is not None:
            with zipfile.ZipFile(asset) as archive:
                archive.extractall(scratch_dir)
            exe = scratch_dir.joinpath(*PurePosixPath(relative_exe).parts)
        else:
            exe = scratch_dir.joinpath(*PurePosixPath(baseline["executable_member"]).parts)
            exe.parent.mkdir(parents=True, exist_ok=True)
            exe.write_bytes(asset.read_bytes())
        executable = select_b1_through_door(source, exe)
        env = {key: value for key, value in os.environ.items()
               if not key.upper().startswith(("NOVA_", "NOVAC_"))}
        env["NOVA_STD_PATH"] = str(source / baseline["std_path"])
        env["NOVAC_SELF_PATH"] = baseline["self_path"]
        result = subprocess.run(
            [str(executable), "emit", "novac/src/main.nv"],
            cwd=source,
            env=env,
            capture_output=True,
            check=False,
        )
        if result.returncode != 0:
            raise VerificationError("RED: B1 emit exited nonzero")
        if result.stderr:
            raise VerificationError("RED: B1 emit stderr is not empty")
        validate_c_observation(result.stdout, baseline)
        (scratch_dir / "emitted.c").write_bytes(result.stdout)
    print(f"PASS: Windows B1 emit exit=0; expected C: {baseline['expected_c_bytes']} bytes SHA-256 {baseline['expected_c_sha256']}; stderr empty")
    print(f"PASS: source content {baseline['source_revision']}; input asset SHA-256 {asset_sha}; B1 SHA-256 {baseline['b1_exe_sha256']}")
    print("PROVENANCE: " + " ".join(baseline["provenance"].values()))
    print("SCOPE: Windows Carina 0.2.1 C-level bootstrap proof; no native EXE/PDB/LIB equality or Linux claim")
    print(f"verify-carina-bootstrap-c ok: pinned B1 emitted the managed C baseline ({baseline['expected_c_bytes']} bytes)")


def select_b1_through_door(source: Path, executable: Path) -> Path:
    lib = Path(__file__).resolve().parents[1] / "guards" / "lib"
    sys.path.insert(0, str(lib))
    from novac_bin import novac_bin

    had_novac = "NOVAC" in os.environ
    previous_novac = os.environ.get("NOVAC")
    os.environ["NOVAC"] = str(executable.resolve())
    try:
        selected = Path(novac_bin(str(source))).resolve()
    finally:
        if had_novac:
            os.environ["NOVAC"] = previous_novac or ""
        else:
            os.environ.pop("NOVAC", None)
    if selected != executable.resolve():
        raise VerificationError("RED: novac_bin did not select the explicit pinned B1 input")
    return selected


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source_tree", type=Path,
                        help="pinned source tree with novac/src and std/src unchanged")
    parser.add_argument("--asset", type=Path, required=True,
                        help="explicit published ZIP or its pinned executable")
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
