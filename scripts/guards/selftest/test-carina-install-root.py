#!/usr/bin/env python3
"""Task 111: fail-closed installed-root probes against the real Windows driver.

Run with an explicit staged package, pinned seed ZIP and per-session scratch.
Only a private copy of the package is mutated; full receipts stay in scratch.
Each missing marker must reject the external project before seed/native work.
Each restored marker must build and run the D89 fixture with exact output.
This is an install test, not a replacement for package-release's ZIP smoke.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[3]
MARKERS = (
    "docs/dev/carina-seeds/windows-x86_64.json",
    "scripts/tools/carina-gate.py",
    "novac/src",
    "nova_rt",
)


def require(condition: bool, message: str) -> None:
    if not condition:
        raise RuntimeError(message)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--driver", type=Path, required=True)
    parser.add_argument("--driver-sha256", required=True)
    parser.add_argument("--install-root", type=Path, required=True)
    parser.add_argument("--seed-zip", type=Path, required=True)
    parser.add_argument("--scratch", type=Path, required=True)
    parser.add_argument("--clang", type=Path, required=True)
    args = parser.parse_args()
    require(os.name == "nt", "this probe requires the Windows Carina seed")
    scratch = args.scratch.resolve(strict=True)
    require(not scratch.is_relative_to(ROOT), "scratch must be outside the checkout")
    driver = args.driver.resolve(strict=True)
    driver_sha = hashlib.sha256(driver.read_bytes()).hexdigest()
    require(driver_sha == args.driver_sha256.lower(), "candidate driver SHA256 mismatch")
    source = args.install_root.resolve(strict=True)
    for marker in MARKERS:
        require((source / marker).exists(), f"source package missing marker: {marker}")

    work = Path(tempfile.mkdtemp(prefix="install-root-probes-", dir=scratch))
    print(f"evidence={work}", flush=True)
    install = work / "install"
    shutil.copytree(source, install)
    shutil.copyfile(driver, install / "nova.exe")
    project = work / "external-project"
    project.mkdir()
    (project / "nova.toml").write_text('[package]\nname = "probe"\n', encoding="utf-8")
    fixture = project / "standalone" / "stdout_hello.nv"
    fixture.parent.mkdir()
    shutil.copyfile(ROOT / "spec_tests/conformance/standalone/stdout_hello.nv", fixture)

    env = os.environ.copy()
    env.update({
        "NOVA_RT_DIR": str(install / "nova_rt"),
        "NOVA_CG_INCLUDE": str(install),
        "NOVA_STD_PATH": str(install / "std"),
        "NOVA_GC_LIB_DIR": str(install / "gc/lib"),
        "NOVA_GC_INCLUDE_DIR": str(install / "gc/include"),
        "NOVA_CARINA_SEED_ZIP": str(args.seed_zip.resolve(strict=True)),
        # A mistaken delegation cannot succeed using the real sibling compat.
        "NOVA_COMPAT_PATH": str(work / "compat-must-not-run.exe"),
    })
    receipts = []

    def build(label: str, green: bool) -> None:
        output = work / f"{label}.exe"
        argv = [str(install / "nova.exe"), "build", str(fixture), "-o", str(output),
                "--toolchain", "clang", "--clang", str(args.clang.resolve(strict=True)),
                "--timeout", "300"]
        result = subprocess.run(argv, cwd=project, env=env, capture_output=True, timeout=420)
        receipt = {"label": label, "argv": argv, "cwd": str(project),
                   "exit": result.returncode, "stdout": result.stdout.decode("utf-8"),
                   "stderr": result.stderr.decode("utf-8"), "output_exists": output.is_file()}
        receipts.append(receipt)
        (work / "receipts.json").write_text(
            json.dumps({"driver_sha256": driver_sha, "runs": receipts}, indent=2) + "\n",
            encoding="utf-8",
        )
        if green:
            require(result.returncode == 0 and output.is_file(), f"{label}: build failed: {receipt}")
            run = subprocess.run([str(output)], cwd=project, capture_output=True, timeout=60)
            require((run.returncode, run.stdout, run.stderr) ==
                    (0, b"hello world from Nova\r\n", b""), f"{label}: D89 output mismatch")
            receipt.update(native_exit=run.returncode, native_stdout=run.stdout.decode("utf-8"),
                           native_stderr=run.stderr.decode("utf-8"))
            print(f'{label}: GREEN build_exit=0 native_exit=0 stdout={run.stdout!r} stderr={run.stderr!r}', flush=True)
        else:
            expected = ("nova: entry is not associated with a Nova workspace or installation: "
                        + str(fixture) + "\n")
            require(result.returncode == 1 and not output.exists(), f"{label}: did not fail closed")
            require(result.stdout == b"" and receipt["stderr"].replace("\r\n", "\n") == expected,
                    f"{label}: unexpected failure: {receipt}")
            print(f'{label}: RED exit=1 stdout=empty output_exists=false stderr={receipt["stderr"]!r}', flush=True)
        (work / "receipts.json").write_text(
            json.dumps({"driver_sha256": driver_sha, "runs": receipts}, indent=2) + "\n",
            encoding="utf-8",
        )

    build("baseline", green=True)
    for index, marker in enumerate(MARKERS, start=1):
        path = install / marker
        hidden = path.with_name(path.name + ".probe-hidden")
        path.rename(hidden)
        try:
            print(f"missing_marker={marker}", flush=True)
            build(f"missing-{index}", green=False)
        finally:
            hidden.rename(path)
        build(f"restored-{index}", green=True)
    print("test-carina-install-root PASS: four missing-marker REDs and four restored D89 GREENs", flush=True)


if __name__ == "__main__":
    main()
